use std::collections::BTreeMap;

use crate::{Result, error::TaffyCanvasError};

pub trait AssetProvider: Send + Sync {
    fn load(&self, key: &str) -> Result<Vec<u8>>;
}

pub trait ResourceProvider: AssetProvider {
    fn fonts(&self) -> &[FontAsset];
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
}

impl MemoryAssetProvider {
    pub fn new(assets: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            assets,
            fonts: Vec::new(),
        }
    }

    pub fn insert_asset(&mut self, key: impl Into<String>, bytes: Vec<u8>) {
        self.assets.insert(key.into(), bytes);
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
}
