use poise::{CreateReply, command};
use serenity::all::*;
use tracing::warn;

use crate::{
    commands::Context, error::BotError, services::license::LicensePublishService,
    utils::LicenseEmbedBuilder,
};

#[command(
    slash_command,
    user_cooldown = 10,
    name_localized("zh-CN", "发布协议"),
    description_localized("zh-CN", "在当前帖子发布协议"),
    ephemeral
)]
/// Publishes the license in the current thread
pub async fn publish_license(
    ctx: Context<'_>,
    #[name_localized("zh-CN", "协议")]
    #[description_localized("zh-CN", "选择要发布的协议")]
    #[autocomplete = "autocomplete_license"]
    license_id: String,

    #[name_localized("zh-CN", "备份权限")]
    #[description_localized("zh-CN", "覆盖协议中的备份权限设置（可选）")]
    backup_override: Option<bool>,
) -> Result<(), BotError> {
    let db = ctx.data().db.clone();

    // 1. 前置安全检查
    // 检查是否在帖子中
    let channel = ctx.channel_id().to_channel(&ctx).await?;
    let is_thread = matches!(
        channel,
        Channel::Guild(GuildChannel {
            kind: ChannelType::PublicThread | ChannelType::PrivateThread | ChannelType::NewsThread,
            ..
        })
    );

    if !is_thread {
        ctx.send(
            CreateReply::default()
                .content("请在您创建的帖子中使用本命令。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    // 获取thread信息
    let thread = channel.guild().unwrap();

    // 检查是否是帖子创建者
    if thread.owner_id != Some(ctx.author().id) {
        ctx.send(
            CreateReply::default()
                .content("您只能为自己创建的帖子添加授权协议。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    // 2. 获取选择的协议
    let license = if let Some(user_id_str) = license_id.strip_prefix("user:") {
        // 用户协议
        let user_id = match user_id_str.parse::<i32>() {
            Ok(id) => id,
            Err(_) => {
                ctx.send(
                    CreateReply::default()
                        .content("无效的协议ID格式。")
                        .ephemeral(true),
                )
                .await?;
                return Ok(());
            }
        };
        let Some(license) = db.license().get_license(user_id, ctx.author().id).await? else {
            ctx.send(
                CreateReply::default()
                    .content("未找到该协议。")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        };
        license
    } else if let Some(system_name) = license_id.strip_prefix("system:") {
        // 系统协议
        let system_licenses = ctx.data().system_license_cache.get_all().await;
        let Some(system_license) = system_licenses
            .iter()
            .find(|l| l.license_name == system_name)
        else {
            ctx.send(
                CreateReply::default()
                    .content("未找到该系统协议。")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        };

        // 将系统协议转换为数据库模型格式
        // 使用一个虚拟的ID，因为这是系统协议
        system_license.to_user_license(ctx.author().id, -1)
    } else {
        ctx.send(
            CreateReply::default()
                .content("无效的协议格式。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };

    // 应用备份权限覆盖
    let backup_allowed = backup_override.unwrap_or(license.allow_backup);

    // 3. 生成预览embed
    let display_name = ctx
        .author_member()
        .await
        .map(|m| m.display_name().to_string())
        .unwrap_or_else(|| ctx.author().name.clone());
    let preview_embed =
        LicenseEmbedBuilder::create_license_embed(&license, backup_allowed, &display_name);

    // 创建按钮
    let publish_btn = CreateButton::new("publish_license")
        .label("✅ 发布")
        .style(ButtonStyle::Success);
    let cancel_btn = CreateButton::new("cancel_publish")
        .label("❌ 取消")
        .style(ButtonStyle::Danger);

    let reply =
        CreateReply::default()
            .embed(preview_embed)
            .components(vec![CreateActionRow::Buttons(vec![
                publish_btn,
                cancel_btn,
            ])]);

    let handler = ctx.send(reply).await?;

    // 4. 等待用户交互
    let Some(interaction) = handler
        .message()
        .await?
        .await_component_interaction(ctx)
        .author_id(ctx.author().id)
        .await
    else {
        warn!("Interaction timed out");
        return Ok(());
    };

    match interaction.data.custom_id.as_str() {
        "publish_license" => {
            interaction
                .create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;

            // 使用统一的发布服务
            LicensePublishService::publish(
                ctx.http(),
                ctx.data(),
                &thread,
                &license,
                backup_allowed,
                ctx.author().id,
                &ctx.author().name,
                &display_name,
            )
            .await?;

            // 更新回复
            handler
                .edit(
                    ctx,
                    CreateReply::default()
                        .embed(LicenseEmbedBuilder::create_license_published_embed(
                            &license.license_name,
                        ))
                        .components(vec![]),
                )
                .await?;
        }
        "cancel_publish" => {
            interaction
                .create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;

            handler
                .edit(
                    ctx,
                    CreateReply::default()
                        .content("已取消发布协议。")
                        .components(vec![]),
                )
                .await?;
        }
        _ => {}
    }

    Ok(())
}

// 自动补全函数
async fn autocomplete_license(
    ctx: Context<'_>,
    partial: &str,
) -> impl Iterator<Item = poise::serenity_prelude::AutocompleteChoice> {
    let db = ctx.data().db.clone();

    // 获取用户的个人协议
    let user_licenses = db
        .license()
        .get_user_licenses(ctx.author().id)
        .await
        .unwrap_or_default();
    let system_licenses = ctx.data().system_license_cache.get_all().await;

    // 组合并过滤
    user_licenses
        .into_iter()
        .map(|l| {
            let name = l.license_name.clone();
            let value = format!("user:{}", l.id);
            (name, value)
        })
        .chain(system_licenses.into_iter().map(|l| {
            let display_name = format!("{} (系统)", l.license_name);
            let value = format!("system:{}", l.license_name);
            (display_name, value)
        }))
        .filter(|(name, _)| name.to_lowercase().contains(&partial.to_lowercase()))
        .take(25)
        .map(|(name, value)| poise::serenity_prelude::AutocompleteChoice::new(name, value))
}
