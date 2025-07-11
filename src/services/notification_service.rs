use std::sync::Arc;
use arc_swap::ArcSwap;
use reqwest::Client;
use serde::Serialize;
use snafu::ResultExt;
use tracing;

use crate::config::BotCfg;
use crate::error::BotError;

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
        let response = self.client
            .post(endpoint.clone())
            .json(payload)
            .send()
            .await
            .whatever_context("发送通知请求时发生网络错误")?;

        // 3. 处理响应
        if response.status().is_success() {
            tracing::info!("成功发送备份通知到 {}", endpoint);
            Ok(())
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "无法读取响应体".to_string());
            tracing::error!("发送备份通知失败，状态码: {}, 响应: {}", status, error_text);
            Err(BotError::GenericError {
                message: format!("通知服务返回错误: {}", status),
                source: None,
            })
        }
    }
}