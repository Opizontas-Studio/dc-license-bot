use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use serenity::all::{ChannelId, Http, MessageId};
use tokio::time;
use tracing::{error, info, warn};

use crate::{config::BotCfg, database::BotDatabase};

/// 启动系统状态监控后台任务
///
/// 如果配置中存在状态消息信息，则启动定时更新任务
pub async fn start_status_monitor(
    http: Arc<Http>,
    db: Arc<BotDatabase>,
    cfg: Arc<ArcSwap<BotCfg>>,
    cache: Arc<serenity::cache::Cache>,
) {
    // 检查配置中是否有状态消息信息
    let config = cfg.load();
    let Some(channel_id) = config.status_message_channel_id else {
        info!("系统状态监控未配置，跳过启动。使用 /setup_system_status 命令进行配置。");
        return;
    };

    let Some(message_id) = config.status_message_id else {
        warn!("系统状态监控配置不完整（缺少 message_id），跳过启动。");
        return;
    };

    let update_interval_secs = config.status_update_interval_secs;
    drop(config); // 释放 config 引用

    info!(
        "启动系统状态监控，频道: {}, 消息: {}, 更新间隔: {} 秒",
        channel_id, message_id, update_interval_secs
    );

    tokio::spawn(async move {
        status_monitor_task(http, db, cfg, cache, channel_id, message_id, update_interval_secs)
            .await;
    });
}

/// 状态监控后台任务
async fn status_monitor_task(
    http: Arc<Http>,
    db: Arc<BotDatabase>,
    _cfg: Arc<ArcSwap<BotCfg>>,
    cache: Arc<serenity::cache::Cache>,
    channel_id: ChannelId,
    message_id: MessageId,
    update_interval_secs: u64,
) {
    let mut interval = time::interval(Duration::from_secs(update_interval_secs));

    // 第一次立即触发会在启动时发生，我们跳过它，等待第一个实际间隔
    interval.tick().await;

    loop {
        interval.tick().await;

        // 使用 ping 模拟延迟（在后台任务中我们无法直接获取 WebSocket 延迟）
        // 这里使用一个固定值或者从其他地方获取
        let latency = Duration::from_millis(100); // 占位值

        match crate::commands::system::create_system_info_embed(&db, &cache, latency).await {
            Ok(embed) => {
                // 编辑消息
                if let Err(e) = http
                    .edit_message(
                        channel_id,
                        message_id,
                        &serenity::all::EditMessage::new().embed(embed),
                        Vec::new(),
                    )
                    .await
                {
                    error!("更新系统状态消息失败: {}", e);
                    // 如果消息不存在或无法访问，可以考虑停止任务
                    if e.to_string().contains("Unknown Message") {
                        error!("状态消息不存在，停止监控任务。");
                        break;
                    }
                }
            }
            Err(e) => {
                error!("创建系统信息 embed 失败: {}", e);
            }
        }
    }
}
