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
    all::{ChannelId, MessageId, RoleId, UserId},
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
    // GRPC网关配置
    pub gateway_enabled: Option<bool>,
    pub gateway_address: Option<String>,
    pub gateway_api_key: Option<String>,
    // 系统状态监控配置
    pub status_message_channel_id: Option<ChannelId>,
    pub status_message_id: Option<MessageId>,
    #[serde(default = "default_status_update_interval")]
    pub status_update_interval_secs: u64,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub bot_start_time: DateTime<Utc>,
}

fn default_status_update_interval() -> u64 {
    60 // 默认60秒更新一次
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
        std::fs::write(&self.path, toml_content)
            .whatever_context("Failed to write configuration file")
    }
}
