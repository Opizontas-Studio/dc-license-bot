use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use serenity::all::{ChannelId, Http, MessageId};
use tokio::{sync::RwLock, task::JoinHandle, time};
use tracing::{error, info, warn};

use crate::{config::BotCfg, database::BotDatabase};

/// 全局的状态监控任务 handle
static STATUS_MONITOR_HANDLE: tokio::sync::OnceCell<RwLock<Option<JoinHandle<()>>>> =
    tokio::sync::OnceCell::const_new();

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

    let handle = tokio::spawn(async move {
        status_monitor_task(http, db, cfg, cache, channel_id, message_id, update_interval_secs)
            .await;
    });

    // 保存任务 handle
    let handle_lock = STATUS_MONITOR_HANDLE
        .get_or_init(|| async { RwLock::new(None) })
        .await;
    *handle_lock.write().await = Some(handle);
}

/// 重启系统状态监控任务
///
/// 会先停止旧任务（如果存在），然后启动新任务
pub async fn restart_status_monitor(
    http: Arc<Http>,
    db: Arc<BotDatabase>,
    cfg: Arc<ArcSwap<BotCfg>>,
    cache: Arc<serenity::cache::Cache>,
) {
    // 停止旧任务
    if let Some(handle_lock) = STATUS_MONITOR_HANDLE.get() {
        let mut handle_guard = handle_lock.write().await;
        if let Some(old_handle) = handle_guard.take() {
            info!("停止旧的系统状态监控任务");
            old_handle.abort();
        }
    }

    // 启动新任务
    start_status_monitor(http, db, cfg, cache).await;
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
    loop {
        // 执行状态更新
        let latency = Duration::from_millis(100);

        match crate::commands::system::create_system_info_embed(&db, &cache, latency).await {
            Ok(embed) => {
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

        // 等待下一次更新
        time::sleep(Duration::from_secs(update_interval_secs)).await;
    }

    warn!("系统状态监控任务已停止");
}
