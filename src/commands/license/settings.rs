use futures::StreamExt;
use poise::{CreateReply, command};
use serenity::all::{
    colours::branding::{GREEN, RED, YELLOW},
    *,
};

use super::super::Context;
use crate::error::BotError;

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
            .get_default_license_id(ctx.author().id)
            .await?;
        let name = if let Some(license) = default_license {
            db.license()
                .get_license(license, ctx.author().id)
                .await?
                .map(|l| l.license_name)
                .unwrap_or_else(|| "未设置".to_string())
        } else {
            "未设置".to_string()
        };
        Ok(CreateEmbed::new()
            .title("🔧 自动发布设置")
            .description("以下是自动发布的设置选项：")
            .field(
                "自动发布",
                auto_copyright.then(|| "启用").unwrap_or_else(|| "禁用"),
                true,
            )
            .field("默认协议", name, true)
            .colour(if auto_copyright { GREEN } else { RED }))
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
                let licenses = db.license().get_user_licenses(ctx.author().id).await?;
                if licenses.is_empty() {
                    handler
                        .edit(
                            ctx,
                            create_reply(
                                create_embed()
                                    .await?
                                    .description("您还没有任何协议，请先创建一个协议。")
                                    .colour(YELLOW),
                            ),
                        )
                        .await?;
                    continue;
                }
                let options = licenses
                    .into_iter()
                    .map(|license| {
                        CreateSelectMenuOption::new(license.license_name, license.id.to_string())
                    })
                    .collect();
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
                        let license_id = if selected == "none" {
                            None
                        } else {
                            selected.parse::<i32>().ok()
                        };

                        db.user_settings()
                            .set_default_license(ctx.author().id, license_id)
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
