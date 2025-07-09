use poise::{CreateReply, Modal, command};
use serenity::all::*;
use tracing::warn;

use super::super::Context;
use crate::error::BotError;

#[derive(Debug, Modal)]
#[name = "编辑协议"]
struct EditLicenseModal {
    #[name = "协议名称"]
    #[placeholder = "输入协议名称"]
    #[min_length = 1]
    #[max_length = 100]
    license_name: String,

    #[name = "允许二传（是/否）"]
    #[placeholder = "是 或 否"]
    #[min_length = 1]
    #[max_length = 2]
    allow_redistribution: String,

    #[name = "允许二改（是/否）"]
    #[placeholder = "是 或 否"]
    #[min_length = 1]
    #[max_length = 2]
    allow_modification: String,

    #[name = "允许备份（是/否）"]
    #[placeholder = "是 或 否"]
    #[min_length = 1]
    #[max_length = 2]
    allow_backup: String,

    #[name = "限制条件（可选）"]
    #[placeholder = "输入限制条件，留空表示无限制"]
    #[max_length = 1000]
    restrictions_note: String,
}
#[command(
    slash_command,
    guild_only,
    owners_only,
    global_cooldown = 10,
    name_localized("zh-CN", "协议管理"),
    description_localized("zh-CN", "管理现有协议"),
    ephemeral
)]
pub async fn license_manager(ctx: Context<'_>) -> Result<(), BotError> {
    let db = ctx.data().db.clone();
    // get the user's licenses from the database
    let licenses = db.license().get_user_licenses(ctx.author().id).await?;
    // if the user has no licenses, send a message and return
    let embed = CreateEmbed::new()
        .title("📜 协议管理")
        .description("选择您要管理的协议：")
        .colour(Colour::DARK_BLUE);
    if licenses.is_empty() {
        let reply = CreateReply::default()
            .embed(embed.field("无协议", "您还没有创建任何协议。", false))
            .ephemeral(true);
        ctx.send(reply).await?;
        return Ok(());
    }
    // create a select menu with the user's licenses
    let options = licenses
        .into_iter()
        .map(|license| CreateSelectMenuOption::new(license.license_name, license.id.to_string()))
        .collect();
    let select_menu =
        CreateSelectMenu::new("select_license", CreateSelectMenuKind::String { options })
            .placeholder("选择要设置的协议")
            .max_values(1);
    // create the reply with the select menu
    let reply = CreateReply::default()
        .embed(embed)
        .components(vec![CreateActionRow::SelectMenu(select_menu)]);
    let reply = ctx.send(reply).await?;
    // wait for the user to select a license
    let Some(itx) = reply
        .message()
        .await?
        .await_component_interaction(ctx)
        .author_id(ctx.author().id)
        .await
    else {
        warn!("Interaction timed out or was not found.");
        return Ok(());
    };
    // validate the interaction data
    let ComponentInteractionDataKind::StringSelect { values } = itx.data.kind.to_owned() else {
        warn!(
            "Expected String kind for select menu, found {:?}",
            itx.data.kind
        );
        return Ok(());
    };
    if values.len() != 1 {
        warn!(
            "Expected exactly one value to be selected, found {}",
            values.len()
        );
        return Ok(());
    }
    let license_id = values[0].parse::<i32>()?;
    // fetch the license from the database
    let Some(license) = db
        .license()
        .get_license(license_id, ctx.author().id)
        .await?
    else {
        warn!(
            "License with ID {} not found for user {}",
            license_id,
            ctx.author().id
        );
        let reply = CreateReply::default()
            .content("未找到该协议。")
            .ephemeral(true);
        ctx.send(reply).await?;
        return Ok(());
    };
    // Acknowledge the first interaction
    itx.create_response(ctx, CreateInteractionResponse::Acknowledge)
        .await?;

    // Create function to generate the second menu embed
    let create_second_menu_embed = |license: &entities::entities::user_licenses::Model| {
        CreateEmbed::new()
            .title(format!("协议名称: {}", license.license_name))
            .description(format!(
                "协议内容:\n\n**二传:** {}\n**二改:** {}\n**限制条件:** {}\n**备份权限:** {}",
                if license.allow_redistribution {
                    "允许"
                } else {
                    "不允许"
                },
                if license.allow_modification {
                    "允许"
                } else {
                    "不允许"
                },
                license.restrictions_note.as_deref().unwrap_or("无"),
                if license.allow_backup {
                    "允许"
                } else {
                    "不允许"
                }
            ))
            .colour(Colour::DARK_BLUE)
    };

    // Create buttons for the second menu
    let delete_btn = CreateButton::new("delete_license")
        .label("删除协议")
        .style(ButtonStyle::Danger);
    let back_btn = CreateButton::new("back")
        .label("返回")
        .style(ButtonStyle::Secondary);
    let exit_btn = CreateButton::new("exit")
        .label("退出")
        .style(ButtonStyle::Secondary);

    // Create the second menu reply
    let second_menu_reply = CreateReply::default()
        .embed(create_second_menu_embed(&license))
        .components(vec![CreateActionRow::Buttons(vec![
            delete_btn.clone(),
            back_btn.clone(),
            exit_btn.clone(),
        ])]);

    // Edit the original message to show the second menu
    reply.edit(ctx, second_menu_reply.clone()).await?;

    // Create interaction stream for the second menu
    let Some(itx) = reply
        .message()
        .await?
        .await_component_interaction(ctx)
        .author_id(ctx.author().id)
        .await
    else {
        warn!("Interaction timed out or was not found.");
        return Ok(());
    };

    match itx.data.custom_id.as_str() {
        "delete_license" => {
            // Acknowledge interaction
            itx.create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;

            // Delete license without confirmation
            db.license().delete(license_id, ctx.author().id).await?;

            // Update message to show deletion success
            reply
                .edit(
                    ctx,
                    CreateReply::default()
                        .embed(
                            CreateEmbed::new()
                                .title("✅ 协议已删除")
                                .description(format!(
                                    "协议 '{}' 已成功删除。",
                                    license.license_name
                                ))
                                .colour(colours::branding::GREEN),
                        )
                        .components(vec![]),
                )
                .await?;
        }
        "back" => {
            // Acknowledge interaction
            itx.create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;
        }
        "exit" => {
            // Acknowledge interaction
            itx.create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;
            reply.delete(ctx).await?;
            // Exit the command
            return Ok(());
        }
        _ => {}
    }
    reply.delete(ctx).await?;
    ctx.rerun().await?;

    Ok(())
}
