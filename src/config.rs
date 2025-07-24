use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};

use arc_swap::ArcSwap;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serenity::{
    all::{ChannelId, RoleId, UserId},
    prelude::TypeMapKey,
};
use snafu::ResultExt;

use crate::error::BotError;

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BotCfg {
    pub time_offset: i32,
    pub token: String,
    pub admin_role_ids: HashSet<RoleId>,
    pub backup_enabled: bool,
    pub endpoint: Url,
    pub extra_admins_ids: HashSet<UserId>,
    #[serde(default)]
    pub allowed_forum_channels: HashSet<ChannelId>,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub bot_start_time: DateTime<Utc>,
}

impl TypeMapKey for BotCfg {
    type Value = Arc<ArcSwap<BotCfg>>;
}

impl BotCfg {
    pub fn read(path: impl AsRef<Path>) -> Result<Self, BotError> {
        Ok(Self {
            path: path.as_ref().to_owned(),
            bot_start_time: Utc::now(),
            ..Figment::new()
                .merge(Toml::file(path))
                .merge(Env::prefixed("DOG_BOT_"))
                .extract_lossy()
                .whatever_context::<&str, BotError>("Failed to read bot configuration")?
        })
    }

    pub fn write(&self) -> Result<(), BotError> {
        let toml_content = toml::to_string_pretty(self)
            .whatever_context::<&str, BotError>("Failed to serialize configuration to TOML")?;
        std::fs::write(&self.path, toml_content).whatever_context("Failed to write configuration file")
    }
}
