use poise::{CreateReply, command};
use serenity::all::*;

use super::{Context, check_admin};
use crate::error::BotError;

#[command(
    slash_command,
    check = "check_admin",
    ephemeral,
    name_localized("zh-CN", "添加论坛"),
    description_localized("zh-CN", "将论坛频道添加到Bot的生效域白名单")
)]
/// Add a forum channel to the allowed list
pub async fn add_forum(
    ctx: Context<'_>,
    #[name_localized("zh-CN", "论坛频道")]
    #[description_localized("zh-CN", "要添加的论坛频道")]
    #[channel_types("Forum")]
    forum_channel: GuildChannel,
) -> Result<(), BotError> {
    let channel_id = forum_channel.id;
    
    // 获取当前配置
    let mut cfg = (**ctx.data().cfg().load()).clone();
    
    // 检查是否已存在
    if cfg.allowed_forum_channels.contains(&channel_id) {
        ctx.send(
            CreateReply::default()
                .content(format!("📋 论坛频道 **{}** 已在白名单中。", forum_channel.name))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }
    
    // 添加到白名单
    cfg.allowed_forum_channels.insert(channel_id);
    
    // 更新配置文件
    cfg.write()?;
    
    // 更新内存中的配置
    ctx.data().cfg().store(cfg.into());
    
    ctx.send(
        CreateReply::default()
            .content(format!("✅ 成功将论坛频道 **{}** 添加到Bot生效域白名单。", forum_channel.name))
            .ephemeral(true),
    )
    .await?;
    
    Ok(())
}

#[command(
    slash_command,
    check = "check_admin",
    ephemeral,
    name_localized("zh-CN", "移除论坛"),
    description_localized("zh-CN", "从Bot的生效域白名单中移除论坛频道")
)]
/// Remove a forum channel from the allowed list
pub async fn remove_forum(
    ctx: Context<'_>,
    #[name_localized("zh-CN", "论坛频道")]
    #[description_localized("zh-CN", "要移除的论坛频道")]
    #[channel_types("Forum")]
    forum_channel: GuildChannel,
) -> Result<(), BotError> {
    let channel_id = forum_channel.id;
    
    // 获取当前配置
    let mut cfg = (**ctx.data().cfg().load()).clone();
    
    // 检查是否存在
    if !cfg.allowed_forum_channels.contains(&channel_id) {
        ctx.send(
            CreateReply::default()
                .content(format!("📋 论坛频道 **{}** 不在白名单中。", forum_channel.name))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }
    
    // 从白名单中移除
    cfg.allowed_forum_channels.remove(&channel_id);
    
    // 更新配置文件
    cfg.write()?;
    
    // 更新内存中的配置
    ctx.data().cfg().store(cfg.into());
    
    ctx.send(
        CreateReply::default()
            .content(format!("✅ 成功从Bot生效域白名单中移除论坛频道 **{}**。", forum_channel.name))
            .ephemeral(true),
    )
    .await?;
    
    Ok(())
}

#[command(
    slash_command,
    check = "check_admin",
    ephemeral,
    name_localized("zh-CN", "论坛列表"),
    description_localized("zh-CN", "显示Bot当前生效域的论坛频道列表")
)]
/// List all allowed forum channels
pub async fn list_forums(ctx: Context<'_>) -> Result<(), BotError> {
    let cfg = ctx.data().cfg.load();
    
    if cfg.allowed_forum_channels.is_empty() {
        ctx.send(
            CreateReply::default()
                .content("📋 当前白名单为空，Bot将在所有论坛频道中工作。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }
    
    let mut forum_info = Vec::new();
    
    for &channel_id in &cfg.allowed_forum_channels {
        match channel_id.to_channel(&ctx.http()).await {
            Ok(Channel::Guild(guild_channel)) => {
                if guild_channel.kind == ChannelType::Forum {
                    forum_info.push(format!(
                        "• **{}** (ID: {})", 
                        guild_channel.name, 
                        channel_id
                    ));
                } else {
                    forum_info.push(format!(
                        "• ⚠️ **{}** (ID: {}) - 不是论坛频道", 
                        guild_channel.name, 
                        channel_id
                    ));
                }
            }
            _ => {
                forum_info.push(format!("• ❌ 频道 ID: {} - 无法访问或已删除", channel_id));
            }
        }
    }
    
    let embed = CreateEmbed::new()
        .title("📋 Bot生效域论坛频道列表")
        .description(format!(
            "以下是Bot当前生效的论坛频道列表 (共 {} 个)：\n\n{}", 
            cfg.allowed_forum_channels.len(),
            forum_info.join("\n")
        ))
        .color(0x00FF00)
        .footer(CreateEmbedFooter::new("只有在这些论坛中创建的帖子才会触发自动发布"));
    
    ctx.send(CreateReply::default().embed(embed).ephemeral(true))
        .await?;
    
    Ok(())
}

#[command(
    slash_command,
    check = "check_admin",
    ephemeral,
    name_localized("zh-CN", "清空论坛白名单"),
    description_localized("zh-CN", "清空所有论坛频道白名单，恢复在所有论坛工作的默认行为")
)]
/// Clear all allowed forum channels (revert to default behavior)
pub async fn clear_forums(ctx: Context<'_>) -> Result<(), BotError> {
    // 获取当前配置
    let mut cfg = (**ctx.data().cfg().load()).clone();
    
    if cfg.allowed_forum_channels.is_empty() {
        ctx.send(
            CreateReply::default()
                .content("📋 白名单已经是空的，Bot当前在所有论坛频道中工作。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }
    
    let count = cfg.allowed_forum_channels.len();
    
    // 清空白名单
    cfg.allowed_forum_channels.clear();
    
    // 更新配置文件
    cfg.write()?;
    
    // 更新内存中的配置
    ctx.data().cfg().store(cfg.into());
    
    ctx.send(
        CreateReply::default()
            .content(format!("✅ 已清空论坛白名单（共 {} 个频道），Bot现在将在所有论坛频道中工作。", count))
            .ephemeral(true),
    )
    .await?;
    
    Ok(())
}