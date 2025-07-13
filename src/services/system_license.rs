use crate::{error::BotError, types::license::SystemLicense};
use arc_swap::ArcSwap;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug)]
pub struct SystemLicenseCache {
    licenses: ArcSwap<Vec<SystemLicense>>,
    path: PathBuf,
}

impl SystemLicenseCache {
    pub async fn new(path: &Path) -> Result<Self, BotError> {
        let content = tokio::fs::read_to_string(path).await?;
        let licenses: Vec<SystemLicense> = serde_json::from_str(&content)?;

        Ok(Self {
            licenses: ArcSwap::from_pointee(licenses),
            path: path.to_path_buf(),
        })
    }

    pub async fn get_all(&self) -> Vec<SystemLicense> {
        Vec::clone(self.licenses.load().as_ref())
    }

    pub async fn get_by_name(&self, name: &str) -> Option<SystemLicense> {
        self.licenses
            .load()
            .iter()
            .find(|l| l.license_name == name)
            .cloned()
    }

    pub async fn reload(&self) -> Result<(), BotError> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let new_licenses: Vec<SystemLicense> = serde_json::from_str(&content)?;

        self.licenses.store(Arc::new(new_licenses));

        Ok(())
    }
}
