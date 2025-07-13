mod auto_publish;
mod ping;

pub use ping::PingHandler;

use crate::{commands::Data, error::BotError};
use serenity::all::{Channel, ChannelType, Context, FullEvent};

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
        {
            if guild_channel.kind == ChannelType::Forum {
                // 处理论坛线程创建事件 - 调用自动发布逻辑
                tracing::info!("Forum thread created: {}", thread.name());
                if let Err(e) = auto_publish::handle_thread_create(ctx, thread, data).await {
                    tracing::error!("Auto publish failed: {}", e);
                }
            }
        }
    }
    Ok(())
}
