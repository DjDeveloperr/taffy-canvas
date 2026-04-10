use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use skia_safe::{Data, Image, image::CachingHint};

use crate::{Result, error::TaffyCanvasError};

pub trait AssetProvider: Send + Sync {
    fn load(&self, key: &str) -> Result<Vec<u8>>;
}

pub trait ResourceProvider: AssetProvider {
    fn fonts(&self) -> &[FontAsset];
    fn load_image(&self, key: &str) -> Result<Image>;
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
}

impl MemoryAssetProvider {
    pub fn new(assets: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            assets,
            fonts: Vec::new(),
            decoded_images: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert_asset(&mut self, key: impl Into<String>, bytes: Vec<u8>) {
        let key = key.into();
        self.assets.insert(key.clone(), bytes);
        self.decoded_images
            .write()
            .expect("decoded image cache lock")
            .remove(&key);
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
}

#[derive(Clone, Debug, Default)]
pub struct FileSystemResourceProvider {
    root: PathBuf,
    fonts: Vec<FontAsset>,
    decoded_images: Arc<RwLock<HashMap<String, Image>>>,
}

impl FileSystemResourceProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            fonts: Vec::new(),
            decoded_images: Arc::new(RwLock::new(HashMap::new())),
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
}
