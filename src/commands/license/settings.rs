use futures::StreamExt;
use poise::{CreateReply, command};
use serenity::all::*;

use super::super::Context;
use crate::{error::BotError, types::license::DefaultLicenseIdentifier, utils::LicenseEmbedBuilder};

#[command(
    slash_command,
    owners_only,
    global_cooldown = 10,
    name_localized("zh-CN", "自动发布设置"),
    description_localized("zh-CN", "编辑自动发布设置"),
    ephemeral
)]
/// Fetches system information
pub async fn auto_publish_settings(ctx: Context<'_>) -> Result<(), BotError> {
    let db = ctx.data().db.clone();
    let create_embed = async || -> Result<CreateEmbed, BotError> {
        let auto_copyright = db
            .user_settings()
            .is_auto_publish_enabled(ctx.author().id)
            .await?;
        let default_license = db
            .user_settings()
            .get_default_license(ctx.author().id)
            .await?;
        let name = match default_license {
            Some(DefaultLicenseIdentifier::User(id)) => {
                db.license()
                    .get_license(id, ctx.author().id)
                    .await?
                    .map(|l| l.license_name)
                    .unwrap_or_else(|| "未设置".to_string())
            }
            Some(DefaultLicenseIdentifier::System(name)) => {
                // Verify the system license exists
                let system_licenses = ctx.data().system_license_cache.get_all().await;
                if system_licenses.iter().any(|l| l.license_name == name) {
                    format!("{} (系统)", name)
                } else {
                    "未设置".to_string()
                }
            }
            None => "未设置".to_string(),
        };
        Ok(LicenseEmbedBuilder::create_auto_publish_settings_embed(auto_copyright, name))
    };
    let enable_btn = CreateButton::new("toggle_auto_publish")
        .label("切换自动发布设置")
        .style(ButtonStyle::Primary);
    let default_license_btn = CreateButton::new("set_default_license")
        .label("设置默认协议")
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
                // Get both user licenses and system licenses
                let user_licenses = db.license().get_user_licenses(ctx.author().id).await?;
                let system_licenses = ctx.data().system_license_cache.get_all().await;
                
                // Create options for both user and system licenses
                let mut options = Vec::new();
                
                // Add user licenses
                for license in user_licenses {
                    options.push(
                        CreateSelectMenuOption::new(
                            license.license_name.clone(), 
                            format!("user:{}", license.id)
                        )
                    );
                }
                
                // Add system licenses
                for license in system_licenses {
                    options.push(
                        CreateSelectMenuOption::new(
                            format!("{} (系统)", license.license_name), 
                            format!("system:{}", license.license_name)
                        )
                    );
                }
                
                if options.is_empty() {
                    handler
                        .edit(
                            ctx,
                            create_reply(LicenseEmbedBuilder::create_settings_no_license_embed()),
                        )
                        .await?;
                    continue;
                }
                let select_menu = CreateSelectMenu::new(
                    "set_default_license_select",
                    CreateSelectMenuKind::String { options },
                )
                .placeholder("选择默认协议")
                .max_values(1);

                let embed = create_embed().await?;
                handler
                    .edit(
                        ctx,
                        CreateReply::default().embed(embed).components(vec![
                            CreateActionRow::SelectMenu(select_menu),
                            CreateActionRow::Buttons(vec![
                                enable_btn.clone(),
                                default_license_btn.clone(),
                                close_btn.clone(),
                            ]),
                        ]),
                    )
                    .await?;
            }
            "set_default_license_select" => {
                if let ComponentInteractionDataKind::StringSelect { values } =
                    &first_interaction.data.kind
                {
                    if let Some(selected) = values.first() {
                        let license = if selected == "none" {
                            None
                        } else if let Some(user_id) = selected.strip_prefix("user:") {
                            user_id.parse::<i32>().ok().map(DefaultLicenseIdentifier::User)
                        } else if let Some(system_name) = selected.strip_prefix("system:") {
                            Some(DefaultLicenseIdentifier::System(system_name.to_string()))
                        } else {
                            None
                        };

                        db.user_settings()
                            .set_default_license(ctx.author().id, license)
                            .await?;

                        first_interaction
                            .create_response(ctx, CreateInteractionResponse::Acknowledge)
                            .await?;

                        let embed = create_embed().await?;
                        handler.edit(ctx, create_reply(embed)).await?;
                    }
                }
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
