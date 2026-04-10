use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use skia_safe::{Data, Image, SamplingOptions, image::CachingHint, surfaces};

use crate::{Result, document::ImageFit, error::TaffyCanvasError};

pub trait AssetProvider: Send + Sync {
    fn load(&self, key: &str) -> Result<Vec<u8>>;
}

pub trait ResourceProvider: AssetProvider {
    fn fonts(&self) -> &[FontAsset];
    fn load_image(&self, key: &str) -> Result<Image>;
    fn load_prepared_image(&self, request: &PreparedImageRequest<'_>) -> Result<Image> {
        let image = self.load_image(request.key)?;
        prepare_image(image, request)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreparedImageKey {
    pub width: u32,
    pub height: u32,
    pub fit: ImageFit,
}

#[derive(Clone, Copy, Debug)]
pub struct PreparedImageRequest<'a> {
    pub key: &'a str,
    pub width: u32,
    pub height: u32,
    pub fit: ImageFit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontAsset {
    pub family: String,
    pub bytes: Vec<u8>,
}

impl FontAsset {
    pub fn new(family: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            family: family.into(),
            bytes,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryAssetProvider {
    assets: BTreeMap<String, Vec<u8>>,
    fonts: Vec<FontAsset>,
    decoded_images: Arc<RwLock<HashMap<String, Image>>>,
    prepared_images: Arc<RwLock<HashMap<(String, PreparedImageKey), Image>>>,
}

impl MemoryAssetProvider {
    pub fn new(assets: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            assets,
            fonts: Vec::new(),
            decoded_images: Arc::new(RwLock::new(HashMap::new())),
            prepared_images: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert_asset(&mut self, key: impl Into<String>, bytes: Vec<u8>) {
        let key = key.into();
        self.assets.insert(key.clone(), bytes);
        self.decoded_images
            .write()
            .expect("decoded image cache lock")
            .remove(&key);
        self.prepared_images
            .write()
            .expect("prepared image cache lock")
            .retain(|(cached_key, _), _| cached_key != &key);
    }

    pub fn register_font(&mut self, family: impl Into<String>, bytes: Vec<u8>) {
        self.fonts.push(FontAsset::new(family, bytes));
    }

    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    pub fn decoded_image_count(&self) -> usize {
        self.decoded_images
            .read()
            .expect("decoded image cache lock")
            .len()
    }

    pub fn prepared_image_count(&self) -> usize {
        self.prepared_images
            .read()
            .expect("prepared image cache lock")
            .len()
    }
}

impl AssetProvider for MemoryAssetProvider {
    fn load(&self, key: &str) -> Result<Vec<u8>> {
        self.assets
            .get(key)
            .cloned()
            .ok_or_else(|| TaffyCanvasError::MissingAsset(key.to_string()))
    }
}

impl ResourceProvider for MemoryAssetProvider {
    fn fonts(&self) -> &[FontAsset] {
        &self.fonts
    }

    fn load_image(&self, key: &str) -> Result<Image> {
        if let Some(image) = self
            .decoded_images
            .read()
            .expect("decoded image cache lock")
            .get(key)
            .cloned()
        {
            return Ok(image);
        }

        let bytes = self
            .assets
            .get(key)
            .ok_or_else(|| TaffyCanvasError::MissingAsset(key.to_string()))?;
        let image = Image::from_encoded(Data::new_copy(bytes))
            .and_then(|image| image.make_raster_image(None, Some(CachingHint::Allow)))
            .ok_or_else(|| TaffyCanvasError::Render(format!("failed to decode image `{key}`")))?;

        let mut cache = self
            .decoded_images
            .write()
            .expect("decoded image cache lock");
        let image = cache
            .entry(key.to_string())
            .or_insert_with(|| image.clone())
            .clone();
        Ok(image)
    }

    fn load_prepared_image(&self, request: &PreparedImageRequest<'_>) -> Result<Image> {
        let cache_key = (
            request.key.to_string(),
            PreparedImageKey {
                width: request.width,
                height: request.height,
                fit: request.fit,
            },
        );
        if let Some(image) = self
            .prepared_images
            .read()
            .expect("prepared image cache lock")
            .get(&cache_key)
            .cloned()
        {
            return Ok(image);
        }

        let image = prepare_image(self.load_image(request.key)?, request)?;
        let mut cache = self
            .prepared_images
            .write()
            .expect("prepared image cache lock");
        let image = cache
            .entry(cache_key)
            .or_insert_with(|| image.clone())
            .clone();
        Ok(image)
    }
}

#[derive(Clone, Debug, Default)]
pub struct FileSystemResourceProvider {
    root: PathBuf,
    fonts: Vec<FontAsset>,
    decoded_images: Arc<RwLock<HashMap<String, Image>>>,
    prepared_images: Arc<RwLock<HashMap<(String, PreparedImageKey), Image>>>,
}

impl FileSystemResourceProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            fonts: Vec::new(),
            decoded_images: Arc::new(RwLock::new(HashMap::new())),
            prepared_images: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn register_font_path(
        &mut self,
        family: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let bytes = fs::read(path.as_ref()).map_err(|error| {
            TaffyCanvasError::Io(format!(
                "failed to read font `{}`: {error}",
                path.as_ref().display()
            ))
        })?;
        self.fonts.push(FontAsset::new(family, bytes));
        Ok(())
    }

    pub fn decoded_image_count(&self) -> usize {
        self.decoded_images
            .read()
            .expect("decoded image cache lock")
            .len()
    }

    pub fn prepared_image_count(&self) -> usize {
        self.prepared_images
            .read()
            .expect("prepared image cache lock")
            .len()
    }

    fn resolve_path(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
}

impl AssetProvider for FileSystemResourceProvider {
    fn load(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.resolve_path(key);
        fs::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                TaffyCanvasError::MissingAsset(key.to_string())
            } else {
                TaffyCanvasError::Io(format!(
                    "failed to read asset `{}`: {error}",
                    path.display()
                ))
            }
        })
    }
}

impl ResourceProvider for FileSystemResourceProvider {
    fn fonts(&self) -> &[FontAsset] {
        &self.fonts
    }

    fn load_image(&self, key: &str) -> Result<Image> {
        if let Some(image) = self
            .decoded_images
            .read()
            .expect("decoded image cache lock")
            .get(key)
            .cloned()
        {
            return Ok(image);
        }

        let bytes = self.load(key)?;
        let image = Image::from_encoded(Data::new_copy(&bytes))
            .and_then(|image| image.make_raster_image(None, Some(CachingHint::Allow)))
            .ok_or_else(|| TaffyCanvasError::Render(format!("failed to decode image `{key}`")))?;

        let mut cache = self
            .decoded_images
            .write()
            .expect("decoded image cache lock");
        let image = cache
            .entry(key.to_string())
            .or_insert_with(|| image.clone())
            .clone();
        Ok(image)
    }

    fn load_prepared_image(&self, request: &PreparedImageRequest<'_>) -> Result<Image> {
        let cache_key = (
            request.key.to_string(),
            PreparedImageKey {
                width: request.width,
                height: request.height,
                fit: request.fit,
            },
        );
        if let Some(image) = self
            .prepared_images
            .read()
            .expect("prepared image cache lock")
            .get(&cache_key)
            .cloned()
        {
            return Ok(image);
        }

        let image = prepare_image(self.load_image(request.key)?, request)?;
        let mut cache = self
            .prepared_images
            .write()
            .expect("prepared image cache lock");
        let image = cache
            .entry(cache_key)
            .or_insert_with(|| image.clone())
            .clone();
        Ok(image)
    }
}

fn prepare_image(image: Image, request: &PreparedImageRequest<'_>) -> Result<Image> {
    if request.width == 0 || request.height == 0 {
        return Err(TaffyCanvasError::Render(
            "prepared image size must be greater than zero".to_string(),
        ));
    }

    let mut surface = surfaces::raster_n32_premul((request.width as i32, request.height as i32))
        .ok_or_else(|| {
            TaffyCanvasError::Render("failed to create prepared image surface".to_string())
        })?;
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);

    let draw_rect = fitted_rect(
        image.width() as f32,
        image.height() as f32,
        request.width as f32,
        request.height as f32,
        request.fit,
    );
    canvas.draw_image_rect_with_sampling_options(
        image,
        None,
        draw_rect,
        SamplingOptions::default(),
        &skia_safe::Paint::default(),
    );

    surface
        .image_snapshot()
        .make_raster_image(None, Some(CachingHint::Allow))
        .ok_or_else(|| TaffyCanvasError::Render("failed to snapshot prepared image".to_string()))
}

fn fitted_rect(
    source_width: f32,
    source_height: f32,
    target_width: f32,
    target_height: f32,
    fit: ImageFit,
) -> skia_safe::Rect {
    match fit {
        ImageFit::Fill => skia_safe::Rect::from_xywh(0.0, 0.0, target_width, target_height),
        ImageFit::Contain | ImageFit::Cover => {
            let scale_x = target_width / source_width;
            let scale_y = target_height / source_height;
            let scale = match fit {
                ImageFit::Contain => scale_x.min(scale_y),
                ImageFit::Cover => scale_x.max(scale_y),
                ImageFit::Fill => unreachable!(),
            };
            let draw_width = source_width * scale;
            let draw_height = source_height * scale;
            let draw_x = (target_width - draw_width) * 0.5;
            let draw_y = (target_height - draw_height) * 0.5;
            skia_safe::Rect::from_xywh(draw_x, draw_y, draw_width, draw_height)
        }
    }
}
