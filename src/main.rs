use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{FixedOffset, Utc};
use clap::Parser;
use dc_bot::{
    Args,
    commands::framework,
    config::BotCfg,
    database::BotDatabase,
    error::BotError,
    handlers::*,
    services::{notification_service::NotificationService, system_license::SystemLicenseCache},
};
use serenity::{Client, all::GatewayIntents};
use tracing_subscriber::{
    EnvFilter,
    fmt::{format::Writer, time::FormatTime},
};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

struct TimeFormatter {
    offset: i32,
}

impl FormatTime for TimeFormatter {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let offset = self.offset;
        let now = Utc::now().with_timezone(
            &FixedOffset::east_opt(offset)
                .expect("Failed to create FixedOffset with the configured time offset"),
        );
        write!(w, "{}", now.format("%Y-%m-%d %H:%M:%S%.3f %Z"))
    }
}

#[tokio::main]
async fn main() -> Result<(), BotError> {
    let cfg = BotCfg::read(&Args::parse().config)?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(true)
        .with_timer(TimeFormatter {
            offset: cfg.time_offset,
        })
        .init();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::privileged();

    let db = BotDatabase::new(&Args::parse().db).await?;
    let cfg = Arc::new(ArcSwap::from_pointee(cfg));

    // Initialize system license cache
    let system_license_cache = Arc::new(
        SystemLicenseCache::new(std::path::Path::new(&Args::parse().default_licenses)).await?,
    );

    // Initialize notification service
    let notification_service = Arc::new(NotificationService::new(cfg.clone()));

    let mut client = Client::builder(&cfg.load().token, intents)
        .cache_settings({
            let mut s = serenity::cache::Settings::default();
            s.max_messages = 0; // Set the maximum number of messages to cache
            s.cache_channels = false; // Disable channel caching
            s.cache_guilds = false; // Disable guild caching
            s.cache_users = false; // Disable user caching
            s
        })
        .type_map_insert::<BotDatabase>(db.to_owned())
        .type_map_insert::<BotCfg>(cfg.to_owned())
        .event_handler(PingHandler)
        .framework(framework(
            db,
            cfg,
            system_license_cache,
            notification_service,
        ))
        .await?;

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform exponential backoff until
    // it reconnects.
    Ok(client.start().await?)
}
