use poise::{CreateReply, command};
use serenity::all::*;
use tracing::warn;

use super::super::Context;
use crate::{
    error::BotError,
    utils::{LicenseEditState, LicenseEmbedBuilder, present_license_editing_panel},
};

#[command(
    slash_command,
    guild_only,
    user_cooldown = 10,
    name_localized("zh-CN", "协议管理"),
    description_localized("zh-CN", "管理现有协议"),
    ephemeral
)]
pub async fn license_manager(ctx: Context<'_>) -> Result<(), BotError> {
    let db = ctx.data().db.clone();
    // get the user's licenses from the database
    let licenses = db.license().get_user_licenses(ctx.author().id).await?;
    // if the user has no licenses, send a message and return
    if licenses.is_empty() {
        let reply = CreateReply::default()
            .embed(LicenseEmbedBuilder::create_no_license_embed())
            .ephemeral(true);
        ctx.send(reply).await?;
        return Ok(());
    }
    let embed = LicenseEmbedBuilder::create_license_manager_embed();
    // create a select menu with the user's licenses
    let options = licenses
        .into_iter()
        .map(|license| CreateSelectMenuOption::new(license.license_name, license.id.to_string()))
        .collect();
    let select_menu =
        CreateSelectMenu::new("select_license", CreateSelectMenuKind::String { options })
            .placeholder("选择要设置的协议")
            .max_values(1);

    let cancel_button = CreateButton::new("cancel_license_selection")
        .label("❌ 取消")
        .style(ButtonStyle::Secondary);

    // create the reply with the select menu and cancel button
    let reply = CreateReply::default().embed(embed).components(vec![
        CreateActionRow::SelectMenu(select_menu),
        CreateActionRow::Buttons(vec![cancel_button]),
    ]);
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
    // 处理取消按钮
    if itx.data.custom_id == "cancel_license_selection" {
        itx.delete_response(&ctx.http()).await?;
        return Ok(());
    }

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
        LicenseEmbedBuilder::create_license_detail_embed(license)
    };

    // Helper function to create buttons without cloning
    let create_action_buttons = || {
        vec![
            CreateButton::new("edit_license")
                .label("编辑协议")
                .style(ButtonStyle::Primary),
            CreateButton::new("delete_license")
                .label("删除协议")
                .style(ButtonStyle::Danger),
            CreateButton::new("back")
                .label("返回")
                .style(ButtonStyle::Secondary),
            CreateButton::new("exit")
                .label("退出")
                .style(ButtonStyle::Secondary),
        ]
    };

    // Create the second menu reply
    let second_menu_reply = CreateReply::default()
        .embed(create_second_menu_embed(&license))
        .components(vec![CreateActionRow::Buttons(create_action_buttons())]);

    // Edit the original message to show the second menu
    reply.edit(ctx, second_menu_reply).await?;

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
        "edit_license" => {
            // 创建编辑状态
            let edit_state = LicenseEditState::from_existing(
                license.license_name.clone(),
                license.allow_redistribution,
                license.allow_modification,
                license.restrictions_note.clone(),
                license.allow_backup,
            );

            // 调用编辑器
            match present_license_editing_panel(
                ctx.serenity_context(),
                ctx.data(),
                &itx,
                edit_state,
            )
            .await
            {
                Ok(Some(final_state)) => {
                    // 用户保存了编辑，更新协议
                    let (
                        name,
                        allow_redistribution,
                        allow_modification,
                        restrictions_note,
                        allow_backup,
                    ) = final_state.to_user_license_fields();

                    match db
                        .license()
                        .update(
                            license_id,
                            ctx.author().id,
                            name,
                            allow_redistribution,
                            allow_modification,
                            restrictions_note,
                            allow_backup,
                        )
                        .await
                    {
                        Ok(Some(updated_license)) => {
                            // 更新成功，重新显示协议详情
                            reply
                                .edit(
                                    ctx,
                                    CreateReply::default()
                                        .embed(LicenseEmbedBuilder::create_license_detail_embed(
                                            &updated_license,
                                        ))
                                        .components(vec![CreateActionRow::Buttons(
                                            create_action_buttons(),
                                        )]),
                                )
                                .await?;
                        }
                        Ok(None) => {
                            // 协议不存在
                            reply
                                .edit(
                                    ctx,
                                    CreateReply::default()
                                        .content("协议不存在或更新失败。")
                                        .components(vec![]),
                                )
                                .await?;
                            return Ok(());
                        }
                        Err(e) => {
                            tracing::error!("更新协议失败: {}", e);
                            reply
                                .edit(
                                    ctx,
                                    CreateReply::default()
                                        .content("更新协议时发生错误。")
                                        .components(vec![]),
                                )
                                .await?;
                            return Ok(());
                        }
                    }
                }
                Ok(None) => {
                    // 用户取消了编辑，重新显示原始协议详情
                    reply
                        .edit(
                            ctx,
                            CreateReply::default()
                                .embed(create_second_menu_embed(&license))
                                .components(vec![
                                    CreateActionRow::Buttons(create_action_buttons()),
                                ]),
                        )
                        .await?;
                }
                Err(e) => {
                    tracing::error!("编辑协议失败: {}", e);
                    reply
                        .edit(
                            ctx,
                            CreateReply::default()
                                .content("编辑协议时发生错误。")
                                .components(vec![]),
                        )
                        .await?;
                    return Ok(());
                }
            }
        }
        "delete_license" => {
            // Acknowledge interaction
            itx.create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;

            // Delete license without confirmation
            db.license().delete(license_id, ctx.author().id).await?;

            if let Some(settings) = db.user_settings().get(ctx.author().id).await?
                && settings.default_user_license_id == Some(license_id) {
                    db.user_settings()
                        .set_default_license(ctx.author().id, None, None)
                        .await?;
                }

            // Update message to show deletion success
            reply
                .edit(
                    ctx,
                    CreateReply::default()
                        .embed(LicenseEmbedBuilder::create_license_deleted_embed(
                            &license.license_name,
                        ))
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
