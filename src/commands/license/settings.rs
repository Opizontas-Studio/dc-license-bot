use futures::StreamExt;
use poise::{CreateReply, command};
use serenity::all::*;

use super::super::Context;
use crate::{
    error::BotError, types::license::DefaultLicenseIdentifier, 
    utils::LicenseEmbedBuilder,
};

#[command(
    slash_command,
    user_cooldown = 10,
    name_localized("zh-CN", "自动发布设置"),
    description_localized("zh-CN", "编辑自动发布设置"),
    ephemeral
)]
/// Fetches system information
pub async fn auto_publish_settings(ctx: Context<'_>) -> Result<(), BotError> {
    let db = ctx.data().db.clone();
    let create_embed = async || -> Result<CreateEmbed, BotError> {
        let user_settings = db.user_settings().get_or_create(ctx.author().id).await?;
        let auto_copyright = user_settings.auto_publish_enabled;
        let skip_confirmation = user_settings.skip_auto_publish_confirmation;
        let default_license = db
            .user_settings()
            .get_default_license(ctx.author().id)
            .await?;
        let name = match default_license {
            Some(DefaultLicenseIdentifier::User(id)) => db
                .license()
                .get_license(id, ctx.author().id)
                .await?
                .map(|l| l.license_name)
                .unwrap_or_else(|| "未设置".to_string()),
            Some(DefaultLicenseIdentifier::System(name)) => {
                // Verify the system license exists
                let system_licenses = ctx.data().system_license_cache.get_all().await;
                if system_licenses.iter().any(|l| l.license_name == name) {
                    format!("{name} (系统)")
                } else {
                    "未设置".to_string()
                }
            }
            None => "未设置".to_string(),
        };
        Ok(LicenseEmbedBuilder::create_auto_publish_settings_embed(
            auto_copyright,
            name,
            skip_confirmation,
        ))
    };
    let enable_btn = CreateButton::new("toggle_auto_publish")
        .label("切换自动发布设置")
        .style(ButtonStyle::Primary);
    let default_license_btn = CreateButton::new("set_default_license")
        .label("设置默认协议")
        .style(ButtonStyle::Secondary);
    let skip_confirmation_btn = CreateButton::new("toggle_skip_confirmation")
        .label("切换跳过确认")
        .style(ButtonStyle::Secondary);
    let close_btn = CreateButton::new("close")
        .label("关闭")
        .style(ButtonStyle::Danger);
    let create_reply = |embed: CreateEmbed| {
        CreateReply::default()
            .embed(embed)
            .components(vec![CreateActionRow::Buttons(vec![
                enable_btn.clone(),
                default_license_btn.clone(),
                skip_confirmation_btn.clone(),
                close_btn.clone(),
            ])])
    };
    let embed = create_embed().await?;

    let reply = create_reply(embed);

    let handler = ctx.send(reply).await?;
    let mut interaction_stream = handler
        .message()
        .await?
        .await_component_interaction(ctx)
        .author_id(ctx.author().id)
        .stream();
    while let Some(first_interaction) = interaction_stream.next().await {
        match first_interaction.data.custom_id.as_str() {
            "toggle_auto_publish" => {
                db.user_settings()
                    .toggle_auto_publish(ctx.author().id)
                    .await?;
                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;
                let embed = create_embed().await?;
                handler.edit(ctx, create_reply(embed)).await?;
            }
            "set_default_license" => {
                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;
                // TODO: 重构后需要重新实现选择菜单逻辑
                let embed = create_embed().await?;
                handler.edit(ctx, create_reply(embed)).await?;
            }
            "set_default_license_select" => {
                // TODO: 重构后需要重新实现选择处理逻辑
                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;
                let embed = create_embed().await?;
                handler.edit(ctx, create_reply(embed)).await?;
            }
            "toggle_skip_confirmation" => {
                db.user_settings()
                    .toggle_skip_confirmation(ctx.author().id)
                    .await?;
                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;
                let embed = create_embed().await?;
                handler.edit(ctx, create_reply(embed)).await?;
            }
            "close" => {
                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;
                handler.delete(ctx).await?;
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
