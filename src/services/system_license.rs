use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

use crate::{error::BotError, types::license::SystemLicense};

#[derive(Debug)]
pub struct SystemLicenseCache {
    licenses: RwLock<Vec<SystemLicense>>,
    path: PathBuf,
}

impl SystemLicenseCache {
    pub async fn new(path: &Path) -> Result<Self, BotError> {
        let content = tokio::fs::read_to_string(path).await?;
        let licenses: Vec<SystemLicense> = serde_json::from_str(&content)?;
        
        Ok(Self {
            licenses: RwLock::new(licenses),
            path: path.to_path_buf(),
        })
    }
    
    pub async fn get_all(&self) -> Vec<SystemLicense> {
        self.licenses.read().await.clone()
    }
    
    pub async fn get_by_name(&self, name: &str) -> Option<SystemLicense> {
        self.licenses.read().await
            .iter()
            .find(|l| l.license_name == name)
            .cloned()
    }
    
    pub async fn reload(&self) -> Result<(), BotError> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let new_licenses: Vec<SystemLicense> = serde_json::from_str(&content)?;
        
        let mut licenses = self.licenses.write().await;
        *licenses = new_licenses;
        
        Ok(())
    }
}