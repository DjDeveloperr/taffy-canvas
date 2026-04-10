use std::collections::BTreeMap;

use crate::{error::TaffyCanvasError, Result};

pub trait AssetProvider: Send + Sync {
    fn load(&self, key: &str) -> Result<Vec<u8>>;
}

#[derive(Clone, Debug, Default)]
pub struct MemoryAssetProvider {
    assets: BTreeMap<String, Vec<u8>>,
}

impl MemoryAssetProvider {
    pub fn new(assets: BTreeMap<String, Vec<u8>>) -> Self {
        Self { assets }
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
