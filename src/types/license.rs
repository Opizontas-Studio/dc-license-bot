use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::error::BotError;
use entities::user_licenses::Model as LicenseModel;
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct License {
    pub license_name: String,
    pub allow_redistribution: bool,
    pub allow_modification: bool,
    pub restrictions_note: Option<String>,
    pub allow_backup: bool,
}

impl From<LicenseModel> for License {
    fn from(model: LicenseModel) -> Self {
        License {
            license_name: model.license_name,
            allow_redistribution: model.allow_redistribution,
            allow_modification: model.allow_modification,
            restrictions_note: model.restrictions_note,
            allow_backup: model.allow_backup,
        }
    }
}

impl License {
    pub fn read_system_licenses() -> Result<Vec<License>, BotError> {
        let path = crate::Args::parse().default_licenses;
        let string = std::fs::read_to_string(path)?;
        Ok(serenity::json::from_str(&string)?)
    }
}
