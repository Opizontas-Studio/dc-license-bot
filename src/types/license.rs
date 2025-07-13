use entities::user_licenses::Model as LicenseModel;
use serde::{Deserialize, Serialize};
use serenity::all::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultLicenseIdentifier {
    User(i32),
    System(String),
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SystemLicense {
    pub license_name: String,
    pub allow_redistribution: bool,
    pub allow_modification: bool,
    pub restrictions_note: Option<String>,
    pub allow_backup: bool,
}

impl From<LicenseModel> for SystemLicense {
    fn from(model: LicenseModel) -> Self {
        SystemLicense {
            license_name: model.license_name,
            allow_redistribution: model.allow_redistribution,
            allow_modification: model.allow_modification,
            restrictions_note: model.restrictions_note,
            allow_backup: model.allow_backup,
        }
    }
}

impl SystemLicense {
    pub fn to_user_license(&self, user_id: UserId, index: i32) -> LicenseModel {
        LicenseModel {
            id: index,
            user_id: user_id.get() as i64,
            license_name: self.license_name.clone(),
            allow_redistribution: self.allow_redistribution,
            allow_modification: self.allow_modification,
            restrictions_note: self.restrictions_note.clone(),
            allow_backup: self.allow_backup,
            usage_count: 0,
            created_at: chrono::Utc::now(),
        }
    }
}
