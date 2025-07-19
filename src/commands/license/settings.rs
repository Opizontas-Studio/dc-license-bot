use futures::StreamExt;
use poise::{CreateReply, command};
use serenity::all::*;

use super::super::Context;
use crate::{
    error::BotError, types::license::DefaultLicenseIdentifier, utils::LicenseEmbedBuilder,
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
        let default_system_license_backup = user_settings.default_system_license_backup;
        let default_license = db
            .user_settings()
            .get_default_license(ctx.author().id)
            .await?;
        let (name, is_system_license) = match default_license {
            Some(DefaultLicenseIdentifier::User(id)) => (db
                .license()
                .get_license(id, ctx.author().id)
                .await?
                .map(|l| l.license_name)
                .unwrap_or_else(|| "未设置".to_string()), false),
            Some(DefaultLicenseIdentifier::System(name)) => {
                // Verify the system license exists
                let system_licenses = ctx.data().system_license_cache.get_all().await;
                if system_licenses.iter().any(|l| l.license_name == name) {
                    (format!("{name} (系统)"), true)
                } else {
                    ("未设置".to_string(), false)
                }
            }
            None => ("未设置".to_string(), false),
        };
        Ok(LicenseEmbedBuilder::create_auto_publish_settings_embed(
            auto_copyright,
            name,
            skip_confirmation,
            is_system_license,
            default_system_license_backup,
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
    let system_backup_btn = CreateButton::new("toggle_system_backup")
        .label("系统协议备份权限")
        .style(ButtonStyle::Secondary);
    let close_btn = CreateButton::new("close")
        .label("关闭")
        .style(ButtonStyle::Danger);
    let create_reply = |embed: CreateEmbed, show_system_backup: bool| {
        let mut buttons = vec![
            enable_btn.clone(),
            default_license_btn.clone(),
            skip_confirmation_btn.clone(),
        ];
        if show_system_backup {
            buttons.push(system_backup_btn.clone());
        }
        buttons.push(close_btn.clone());
        
        CreateReply::default()
            .embed(embed)
            .components(vec![CreateActionRow::Buttons(buttons)])
    };
    let embed = create_embed().await?;
    let default_license = db.user_settings().get_default_license(ctx.author().id).await?;
    let is_system_license = matches!(default_license, Some(DefaultLicenseIdentifier::System(_)));

    let reply = create_reply(embed, is_system_license);

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
                let default_license = db.user_settings().get_default_license(ctx.author().id).await?;
                let is_system_license = matches!(default_license, Some(DefaultLicenseIdentifier::System(_)));
                handler.edit(ctx, create_reply(embed, is_system_license)).await?;
            }
            "set_default_license" => {
                // 获取用户协议和系统协议
                let user_licenses = db.license().get_user_licenses(ctx.author().id).await?;
                let system_licenses = ctx.data().system_license_cache.get_all().await;

                // 创建选择菜单选项
                let mut select_options = Vec::new();

                // 添加"无默认协议"选项
                select_options.push(
                    CreateSelectMenuOption::new("无默认协议", "none").description("不设置默认协议"),
                );

                // 添加用户协议选项
                for license in user_licenses {
                    select_options.push(
                        CreateSelectMenuOption::new(
                            &license.license_name,
                            format!("user_{}", license.id),
                        )
                        .description("用户协议"),
                    );
                }

                // 添加系统协议选项
                for license in system_licenses {
                    select_options.push(
                        CreateSelectMenuOption::new(
                            &license.license_name,
                            format!("system_{}", license.license_name),
                        )
                        .description("系统协议"),
                    );
                }

                // 创建选择菜单
                let select_menu = CreateSelectMenu::new(
                    "set_default_license_select",
                    CreateSelectMenuKind::String {
                        options: select_options,
                    },
                )
                .placeholder("请选择默认协议")
                .max_values(1);

                // 创建带有选择菜单的回复
                let reply_with_select = CreateReply::default()
                    .embed(create_embed().await?)
                    .components(vec![CreateActionRow::SelectMenu(select_menu)]);

                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;

                handler.edit(ctx, reply_with_select).await?;
            }
            "set_default_license_select" => {
                // 处理选择菜单的选择
                if let ComponentInteractionDataKind::StringSelect { values } =
                    &first_interaction.data.kind
                {
                    if let Some(selected) = values.first() {
                        let result = if selected == "none" {
                            // 清除默认协议
                            db.user_settings()
                                .set_default_license(ctx.author().id, None, None)
                                .await
                        } else if let Some(user_id) = selected.strip_prefix("user_") {
                            // 设置用户协议为默认
                            if let Ok(license_id) = user_id.parse::<i32>() {
                                db.user_settings()
                                    .set_default_license(
                                        ctx.author().id,
                                        Some(DefaultLicenseIdentifier::User(license_id)),
                                        None,
                                    )
                                    .await
                            } else {
                                Err(BotError::GenericError {
                                    message: "无效的协议ID".to_string(),
                                    source: None,
                                })
                            }
                        } else if let Some(system_name) = selected.strip_prefix("system_") {
                            // 设置系统协议为默认
                            db.user_settings()
                                .set_default_license(
                                    ctx.author().id,
                                    Some(DefaultLicenseIdentifier::System(system_name.to_string())),
                                    None,
                                )
                                .await
                        } else {
                            Err(BotError::GenericError {
                                message: "无效的选择".to_string(),
                                source: None,
                            })
                        };

                        match result {
                            Ok(_) => {
                                first_interaction
                                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                                    .await?;
                                // 返回到主菜单
                                let embed = create_embed().await?;
                                let default_license = db.user_settings().get_default_license(ctx.author().id).await?;
                let is_system_license = matches!(default_license, Some(DefaultLicenseIdentifier::System(_)));
                handler.edit(ctx, create_reply(embed, is_system_license)).await?;
                            }
                            Err(e) => {
                                tracing::error!("设置默认协议失败: {}", e);
                                first_interaction
                                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                                    .await?;
                                let embed = create_embed().await?;
                                let default_license = db.user_settings().get_default_license(ctx.author().id).await?;
                let is_system_license = matches!(default_license, Some(DefaultLicenseIdentifier::System(_)));
                handler.edit(ctx, create_reply(embed, is_system_license)).await?;
                            }
                        }
                    }
                } else {
                    first_interaction
                        .create_response(ctx, CreateInteractionResponse::Acknowledge)
                        .await?;
                    let embed = create_embed().await?;
                    let default_license = db.user_settings().get_default_license(ctx.author().id).await?;
                let is_system_license = matches!(default_license, Some(DefaultLicenseIdentifier::System(_)));
                handler.edit(ctx, create_reply(embed, is_system_license)).await?;
                }
            }
            "toggle_skip_confirmation" => {
                db.user_settings()
                    .toggle_skip_confirmation(ctx.author().id)
                    .await?;
                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;
                let embed = create_embed().await?;
                let default_license = db.user_settings().get_default_license(ctx.author().id).await?;
                let is_system_license = matches!(default_license, Some(DefaultLicenseIdentifier::System(_)));
                handler.edit(ctx, create_reply(embed, is_system_license)).await?;
            }
            "toggle_system_backup" => {
                // 获取当前设置
                let user_settings = db.user_settings().get_or_create(ctx.author().id).await?;
                let current_backup = user_settings.default_system_license_backup;
                
                // 切换备份权限设置
                let new_backup = match current_backup {
                    None => Some(true),      // 未设置 -> 允许备份
                    Some(true) => Some(false), // 允许备份 -> 不允许备份
                    Some(false) => None,      // 不允许备份 -> 使用系统默认
                };
                
                // 更新设置
                db.user_settings()
                    .set_default_license(
                        ctx.author().id,
                        None,  // 不改变默认协议
                        new_backup,
                    )
                    .await?;
                
                first_interaction
                    .create_response(ctx, CreateInteractionResponse::Acknowledge)
                    .await?;
                let embed = create_embed().await?;
                let default_license = db.user_settings().get_default_license(ctx.author().id).await?;
                let is_system_license = matches!(default_license, Some(DefaultLicenseIdentifier::System(_)));
                handler.edit(ctx, create_reply(embed, is_system_license)).await?;
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
