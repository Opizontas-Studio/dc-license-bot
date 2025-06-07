use chrono::Utc;
use owo_colors::OwoColorize;
use serenity::all::{EditMessage, GuildId, Ready};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::prelude::*;
use tracing::{info, warn};

pub struct PingHandler;

#[async_trait]
impl EventHandler for PingHandler {
    // Set a handler for the `message` event. This is called whenever a new message is received.
    //
    // Event handlers are dispatched through a threadpool, and so multiple events can be
    // dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        match msg.content.as_str() {
            "!ping" => {
                let now = Utc::now();
                let msg_time = msg.timestamp.to_utc();
                let delta_one = now - msg_time;
                let reply = format!(
                    "Pong!\nReceive Latency: {} ms",
                    delta_one.num_milliseconds()
                );
                match msg.reply(&ctx.http, reply).await {
                    Ok(mut msg) => {
                        let reply_time = msg.timestamp.to_utc();
                        let delta_two = reply_time - msg_time;
                        msg.edit(
                            &ctx.http,
                            EditMessage::new().content(format!(
                                "Pong!\nReceive Latency: {} ms\nReply Latency: {} ms",
                                delta_one.num_milliseconds(),
                                delta_two.num_milliseconds()
                            )),
                        )
                        .await
                        .unwrap_or_else(|why| {
                            warn!("Error editing message: {why:?}");
                        });
                    }
                    Err(why) => {
                        warn!("Error sending pong message: {why:?}");
                    }
                }
            }
            "!help" => {
                if let Err(why) = msg
                    .channel_id
                    .say(&ctx.http, "狗 Bot!\nWritten in Rust using Serenity!")
                    .await
                {
                    warn!("Error sending help message: {why:?}");
                }
            }
            _ => {}
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        // This is called when the bot is ready and has connected to Discord.
        // You can use this to set the bot's activity or status.
        info!("{} is connected!", ready.user.name.green());
    }

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        // This is called when the cache is ready.
        // list all guilds the bot is in
        info!(
            "Cache is ready! Bot is in {} guilds.",
            guilds.len().to_string().green()
        );
        for guild in guilds {
            let guild_name = ctx
                .cache
                .guild(guild)
                .map(|g| g.name.clone())
                .unwrap_or("Uncached Guild".to_string());
            info!("Connected to: {} ({})", guild_name.green(), guild);
        }
    }
}
