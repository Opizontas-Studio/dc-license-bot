mod auto_publish;
mod auto_publish_flow;
mod ping;

pub use ping::PingHandler;
use serenity::all::{Channel, ChannelType, Context, FullEvent};

use crate::{commands::Data, error::BotError};

pub async fn poise_event_handler(
    ctx: &Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, BotError>,
    data: &Data,
) -> Result<(), BotError> {
    if let FullEvent::ThreadCreate { thread } = event {
        // 检查是否是论坛类型频道中的线程
        if let Ok(Channel::Guild(guild_channel)) = thread
            .parent_id
            .unwrap_or_default()
            .to_channel(&ctx.http)
            .await
            && guild_channel.kind == ChannelType::Forum {
                // 检查论坛频道是否在白名单中
                let cfg = data.cfg().load();
                let is_allowed = cfg.allowed_forum_channels.is_empty() 
                    || cfg.allowed_forum_channels.contains(&guild_channel.id);
                
                if is_allowed {
                    // 处理论坛线程创建事件 - 调用自动发布逻辑
                    tracing::info!("Forum thread created in allowed channel: {}", thread.name());
                    if let Err(e) = auto_publish::handle_thread_create(ctx, thread, data).await {
                        tracing::error!("Auto publish failed: {}", e);
                    }
                } else {
                    tracing::debug!(
                        "Forum thread created in non-allowed channel '{}' (ID: {}), skipping auto publish",
                        guild_channel.name,
                        guild_channel.id
                    );
                }
            }
    }
    Ok(())
}
