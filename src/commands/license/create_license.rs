use poise::{CreateReply, Modal, command};
use serenity::all::*;
use tracing::warn;

use super::super::Context;
use crate::{error::BotError, utils::LicenseEmbedBuilder};

#[derive(Modal)]
#[name = "限制条件"]
struct LicenseModal {
    #[name = "限制条件"]
    #[placeholder = "输入限制条件"]
    #[max_length = 1000]
    restrictions: String,
}

#[command(
    slash_command,
    guild_only,
    user_cooldown = 10,
    name_localized("zh-CN", "创建协议"),
    description_localized("zh-CN", "创建一个新的协议"),
    ephemeral
)]
pub async fn create_license(
    ctx: Context<'_>,
    #[name_localized("zh-CN", "名称")]
    #[description_localized("zh-CN", "协议名称")]
    name: String,

    #[name_localized("zh-CN", "二传")]
    #[description_localized("zh-CN", "是否允许社区内二次传播")]
    redis: bool,
    #[name_localized("zh-CN", "二改")]
    #[description_localized("zh-CN", "是否允许社区内二次修改")]
    modify: bool,
    #[name_localized("zh-CN", "限制条件")]
    #[description_localized("zh-CN", "是否限制条件(可选)")]
    rest: Option<bool>,
    #[name_localized("zh-CN", "备份权限")]
    #[description_localized("zh-CN", "是否允许备份(默认为否)")]
    backup: Option<bool>,
) -> Result<(), BotError> {
    let Context::Application(app_ctx) = ctx else {
        panic!("Context is not an ApplicationContext");
    };
    let modal_resp = if rest == Some(true) {
        let Some(modal_resp) = LicenseModal::execute(app_ctx).await? else {
            warn!("Modal response is None");
            return Ok(());
        };
        Some(modal_resp)
    } else {
        None
    };
    let preview_license_embed = LicenseEmbedBuilder::create_license_preview_embed(
        &name,
        redis,
        modify,
        modal_resp.as_ref().map(|m| m.restrictions.as_str()),
        backup,
    );
    let save_btn = CreateButton::new("save_license")
        .label("保存协议")
        .style(ButtonStyle::Primary);
    let reply = CreateReply::default()
        .embed(preview_license_embed)
        .components(vec![CreateActionRow::Buttons(vec![save_btn])]);
    let reply = ctx.send(reply).await?;
    let Some(itx) = reply
        .message()
        .await?
        .await_component_interactions(ctx)
        .author_id(ctx.author().id)
        .await
    else {
        warn!("No interaction received for the reply");
        return Ok(());
    };
    match itx.data.custom_id.as_str() {
        "save_license" => {
            let result = ctx
                .data()
                .db
                .license()
                .create(
                    ctx.author().id,
                    name,
                    redis,
                    modify,
                    modal_resp.map(|m| m.restrictions),
                    backup.unwrap_or(false),
                )
                .await;

            match result {
                Ok(_) => {
                    itx.create_response(ctx, CreateInteractionResponse::Acknowledge)
                        .await?;
                    reply
                        .edit(
                            ctx,
                            CreateReply::default()
                                .content("协议已创建")
                                .components(vec![]),
                        )
                        .await?;
                }
                Err(BotError::GenericError { message, .. }) => {
                    itx.create_response(ctx, CreateInteractionResponse::Acknowledge)
                        .await?;
                    reply
                        .edit(
                            ctx,
                            CreateReply::default().content(&message).components(vec![]),
                        )
                        .await?;
                }
                Err(e) => return Err(e),
            }
        }
        _ => {
            warn!("Unknown custom_id: {}", itx.data.custom_id);
        }
    }

    Ok(())
}
