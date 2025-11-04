use futures::{StreamExt, stream::FuturesOrdered};
use poise::{CreateReply, command};
use serenity::all::{
    colours::branding::{GREEN, RED, YELLOW},
    *,
};
use sysinfo::System;

use super::{Context, check_admin};
use crate::error::BotError;

/// 创建系统信息 Embed
/// 可被命令和后台服务复用
pub async fn create_system_info_embed(
    db: &crate::database::BotDatabase,
    cache: &serenity::cache::Cache,
    latency: std::time::Duration,
) -> Result<CreateEmbed, BotError> {
    use tikv_jemalloc_ctl::{epoch, stats};
    let kernel_version = System::kernel_long_version();
    let os_version = System::long_os_version().unwrap_or_else(|| "Unknown".into());
    let e = epoch::mib()?;
    let allocated = stats::allocated::mib()?;
    e.advance()?;
    let allocated_value = allocated.read()?;
    let allocated_mb = allocated_value / 1024 / 1024; // Convert to MB
    let sys = System::new_all();
    let cpu = sys.cpus().len().to_string();
    let cpu_usage = sys.global_cpu_usage();
    let total_memory = sys.total_memory() / 1024 / 1024; // Convert to MB
    let used_memory = sys.used_memory() / 1024 / 1024; // Convert to MB
    let memory_usage = (used_memory as f64 / total_memory as f64) * 100.0;
    let rust_version = compile_time::rustc_version_str!();
    let db_size = db.size().await? / 1024 / 1024; // Convert to MB
    let metrics = tokio::runtime::Handle::current().metrics();
    let queue_count = metrics.global_queue_depth();
    let active_count = metrics.num_alive_tasks();
    let workers = metrics.num_workers();

    // Get application statistics
    let auto_publish_users = db
        .user_settings()
        .get_auto_publish_count()
        .await
        .unwrap_or(0);
    let total_posts = db.published_posts().get_total_count().await.unwrap_or(0);
    let backup_allowed_posts = db
        .published_posts()
        .get_backup_allowed_count()
        .await
        .unwrap_or(0);

    // Get color based on CPU usage
    let color = if cpu_usage < 50.0 {
        GREEN // Green
    } else if cpu_usage < 80.0 {
        YELLOW // Yellow
    } else {
        RED // Red
    };

    let embed = CreateEmbed::new()
        .title("🖥️ 系统信息")
        .color(color)
        // row 0
        .field("📟 OS 版本", &os_version, true)
        .field("🔧 内核版本", &kernel_version, true)
        .field("🦀 Rust 版本", rust_version, true)
        // row 1
        .field("🔳 CPU 数量", cpu, true)
        .field("🔥 CPU 使用率", format!("{cpu_usage:.1}%"), true)
        .field(
            "🧠 系统内存",
            format!("{memory_usage:.1}% ({used_memory} MB / {total_memory} MB)"),
            true,
        )
        // row 2
        .field("💭 Bot 内存", format!("{allocated_mb} MB"), true)
        .field("⛁ 数据库大小", format!("{db_size} MB"), true)
        .field(
            "⏱️ WebSocket 延迟",
            format!("{} ms", latency.as_millis()),
            true,
        )
        // row 3
        .field("🚦 Tokio 队列任务", queue_count.to_string(), true)
        .field("🚀 Tokio 活跃任务", active_count.to_string(), true)
        .field("🛠️ Tokio 工作线程", workers.to_string(), true)
        // row 4
        .field("🚀 自动发布用户", auto_publish_users.to_string(), true)
        .field("📄 使用协议作品", total_posts.to_string(), true)
        .field("💾 授权备份作品", backup_allowed_posts.to_string(), true)
        .thumbnail(cache.current_user().avatar_url().unwrap_or_default())
        .timestamp(chrono::Utc::now())
        .footer(CreateEmbedFooter::new("系统监控"))
        .author(CreateEmbedAuthor::from(User::from(
            cache.current_user().clone(),
        )));

    Ok(embed)
}

#[command(
    slash_command,
    default_member_permissions = "ADMINISTRATOR",
    owners_only,
    global_cooldown = 10,
    name_localized("zh-CN", "系统信息"),
    description_localized("zh-CN", "获取系统信息，包括系统名称、内核版本和操作系统版本"),
    ephemeral
)]
/// Fetches system information
pub async fn system_info(ctx: Context<'_>, ephemeral: Option<bool>) -> Result<(), BotError> {
    let ephemeral = ephemeral.unwrap_or(true);
    let latency = ctx.ping().await;

    let embed = create_system_info_embed(ctx.data().db(), ctx.cache(), latency).await?;

    ctx.send(CreateReply::default().embed(embed).ephemeral(ephemeral))
        .await?;

    Ok(())
}

#[command(
    slash_command,
    default_member_permissions = "ADMINISTRATOR",
    owners_only,
    ephemeral
)]
pub async fn guilds_info(ctx: Context<'_>) -> Result<(), BotError> {
    let guild_ids = ctx.cache().guilds();
    // print guilds info, and bot permissions in each guild
    let message = guild_ids
        .into_iter()
        .map(async |guild_id| {
            let guild = ctx.cache().guild(guild_id).map(|g| g.to_owned())?;
            let user_id = ctx.cache().current_user().id;
            let member = guild.member(ctx, user_id).await.ok()?;
            let permissions =
                guild.user_permissions_in(guild.default_channel(member.user.id)?, &member);

            Some(format!(
                "Guild: {}\nPermissions: {}\n\n",
                guild.name,
                permissions.get_permission_names().join(", ")
            ))
        })
        .collect::<FuturesOrdered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");

    if message.is_empty() {
        ctx.say("没有找到任何服务器信息。").await?;
        return Ok(());
    }
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .title("Guilds Information")
                .description(message)
                .color(0x00FF00),
        ),
    )
    .await?;
    Ok(())
}

#[command(
    slash_command,
    default_member_permissions = "ADMINISTRATOR",
    check = "check_admin",
    ephemeral,
    name_localized("zh-CN", "重载系统授权"),
    description_localized("zh-CN", "从配置文件重新加载系统授权协议")
)]
/// Reload system licenses from the configuration file
pub async fn reload_licenses(ctx: Context<'_>) -> Result<(), BotError> {
    let system_license_cache = ctx.data().system_license_cache();

    match system_license_cache.reload().await {
        Ok(()) => {
            ctx.say("✅ 系统授权已成功从文件刷新。").await?;
        }
        Err(error) => {
            let user_message = error.operation_message("reload_licenses");
            let suggestion = error.user_suggestion();

            let content = if let Some(suggestion) = suggestion {
                format!("❌ {user_message}\n💡 {suggestion}")
            } else {
                format!("❌ {user_message}")
            };

            ctx.say(content).await?;
        }
    }

    Ok(())
}

#[command(
    slash_command,
    default_member_permissions = "ADMINISTRATOR",
    owners_only,
    name_localized("zh-CN", "设置系统状态"),
    description_localized("zh-CN", "在当前频道设置自动更新的系统状态消息"),
    ephemeral
)]
/// Setup auto-updating system status message in the current channel
pub async fn setup_system_status(ctx: Context<'_>) -> Result<(), BotError> {
    // 获取当前频道 ID
    let channel_id = ctx.channel_id();

    // 检查是否已有旧的状态消息，如果有则删除
    let current_cfg = ctx.data().cfg().load();
    if let (Some(old_channel_id), Some(old_message_id)) = (
        current_cfg.status_message_channel_id,
        current_cfg.status_message_id,
    ) {
        // 尝试删除旧消息（忽略错误，可能消息已被手动删除）
        let _ = ctx
            .serenity_context()
            .http
            .delete_message(old_channel_id, old_message_id, None)
            .await;
    }
    drop(current_cfg); // 释放引用

    // 创建系统信息 embed
    let latency = ctx.ping().await;
    let embed = create_system_info_embed(ctx.data().db(), ctx.cache(), latency).await?;

    // 在当前频道发送非 ephemeral 消息
    let message = channel_id
        .send_message(
            &ctx.serenity_context().http,
            serenity::all::CreateMessage::new().embed(embed),
        )
        .await?;

    // 更新配置
    let mut cfg = ctx.data().cfg().load().as_ref().clone();
    cfg.status_message_channel_id = Some(channel_id);
    cfg.status_message_id = Some(message.id);

    // 写入配置文件
    cfg.write()?;

    // 更新内存中的配置
    ctx.data().cfg().store(std::sync::Arc::new(cfg));

    // 重启状态监控任务，使用新的配置
    crate::services::status_monitor::restart_status_monitor(
        ctx.serenity_context().http.clone(),
        std::sync::Arc::new(ctx.data().db().clone()),
        ctx.data().cfg().clone(),
        ctx.serenity_context().cache.clone(),
    )
    .await;

    // 向用户发送确认消息（ephemeral）
    ctx.send(
        CreateReply::default()
            .content(format!(
                "✅ 系统状态消息已设置在 <#{}>！\n\
                消息将每 {} 秒自动更新一次。\n\
                监控任务已重启。",
                channel_id,
                ctx.data().cfg().load().status_update_interval_secs
            ))
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
