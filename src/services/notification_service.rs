use std::sync::Arc;

use arc_swap::ArcSwap;
use reqwest::Client;
use serde::Serialize;
use snafu::ResultExt;
use tracing;

use crate::{config::BotCfg, error::BotError};

#[derive(Serialize, Debug)]
pub struct NotificationPayload {
    pub event_type: String,
    pub timestamp: String,
    pub guild_id: String,
    pub channel_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub author: Author,
    pub work_info: WorkInfo,
    pub urls: Urls,
}

#[derive(Serialize, Debug)]
pub struct Author {
    pub discord_user_id: String,
    pub username: String,
    pub display_name: String,
}

#[derive(Serialize, Debug)]
pub struct WorkInfo {
    pub title: String,
    pub content_preview: String,
    pub license_type: String,
    pub backup_allowed: bool,
}

#[derive(Serialize, Debug)]
pub struct Urls {
    pub discord_thread: String,
    pub direct_message: String,
}

#[derive(Debug)]
pub struct NotificationService {
    client: Client,
    config: Arc<ArcSwap<BotCfg>>,
}

impl NotificationService {
    pub fn new(config: Arc<ArcSwap<BotCfg>>) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// 发送备份权限变更的通知
    pub async fn send_backup_notification(
        &self,
        payload: &NotificationPayload,
    ) -> Result<(), BotError> {
        let config = self.config.load();

        // 1. 检查功能是否启用
        if !config.backup_enabled {
            tracing::info!("备份通知功能已禁用，跳过发送。");
            return Ok(());
        }

        let endpoint = &config.endpoint;

        tracing::info!("正在向 {} 发送备份通知...", endpoint);

        // 2. 发送 POST 请求
        let response = self
            .client
            .post(endpoint.clone())
            .json(payload)
            .send()
            .await
            .whatever_context::<&str, BotError>("发送通知请求时发生网络错误")?;

        // 3. 处理响应
        if response.status().is_success() {
            tracing::info!("成功发送备份通知到 {}", endpoint);
            Ok(())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "无法读取响应体".to_string());
            tracing::error!("发送备份通知失败，状态码: {}, 响应: {}", status, error_text);
            Err(BotError::GenericError {
                message: format!("HTTP {}", status.as_u16()),
                source: None,
            })
        }
    }
}

// 通知载荷构造辅助函数
impl NotificationPayload {
    /// 从Discord上下文创建通知载荷
    pub async fn from_discord_context(
        thread: &serenity::all::GuildChannel,
        message_id: serenity::all::MessageId,
        author: serenity::all::User,
        content_preview: String,
        license_type: String,
        backup_allowed: bool,
    ) -> Self {
        let guild_id_str = thread.guild_id.to_string();
        let channel_id_str = thread.parent_id.unwrap_or_default().to_string();
        let thread_id_str = thread.id.to_string();
        let message_id_str = message_id.to_string();

        // 构造 URLs
        let discord_thread_url =
            format!("https://discord.com/channels/{guild_id_str}/{channel_id_str}/{thread_id_str}");
        let direct_message_url =
            format!("https://discord.com/channels/{guild_id_str}/{thread_id_str}/{message_id_str}");

        Self {
            event_type: "backup_permission_update".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            guild_id: guild_id_str,
            channel_id: channel_id_str,
            thread_id: thread_id_str,
            message_id: message_id_str,
            author: Author {
                discord_user_id: author.id.to_string(),
                username: author.name.clone(),
                display_name: author.display_name().to_string(),
            },
            work_info: WorkInfo {
                title: thread.name.clone(),
                content_preview: content_preview.chars().take(100).collect(),
                license_type,
                backup_allowed,
            },
            urls: Urls {
                discord_thread: discord_thread_url,
                direct_message: direct_message_url,
            },
        }
    }
}
