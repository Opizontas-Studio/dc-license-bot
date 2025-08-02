use poise::{CreateReply, command};
use serenity::all::*;
use tracing::warn;

use super::super::Context;
use crate::{
    error::BotError,
    utils::{LicenseEditState, LicenseEmbedBuilder, present_license_editing_panel, UserFriendlyErrorMapper},
};

#[command(
    slash_command,
    guild_only,
    user_cooldown = 10,
    name_localized("zh-CN", "创建协议"),
    description_localized("zh-CN", "创建新协议"),
    ephemeral
)]
pub async fn create_license_interactive(ctx: Context<'_>) -> Result<(), BotError> {
    // 创建一个简单的确认消息来获取ComponentInteraction
    let start_button = CreateButton::new("start_create_license")
        .label("开始创建")
        .style(ButtonStyle::Primary);
    
    let embed = CreateEmbed::new()
        .title("📝 创建新协议")
        .description("使用按钮创建自定义协议。您可以设置协议名称、权限选项和限制条件。")
        .color(0x3498db)
        .footer(CreateEmbedFooter::new("点击下方按钮开始创建"));

    let reply = CreateReply::default()
        .embed(embed)
        .components(vec![CreateActionRow::Buttons(vec![start_button])]);
    
    let reply_handle = ctx.send(reply).await?;
    
    // 等待用户点击按钮
    let Some(interaction) = reply_handle
        .message()
        .await?
        .await_component_interaction(ctx)
        .author_id(ctx.author().id)
        .timeout(std::time::Duration::from_secs(300))
        .await
    else {
        warn!("用户没有响应创建协议面板");
        return Ok(());
    };
    
    if interaction.data.custom_id != "start_create_license" {
        return Ok(());
    }
    
    // 创建初始编辑状态
    let initial_state = LicenseEditState::new("新协议".to_string());
    
    // 调用现有的编辑面板
    if let Ok(Some(final_state)) = present_license_editing_panel(
        ctx.serenity_context(),
        ctx.data(),
        &interaction,
        initial_state,
    )
    .await
    {
        // 用户保存了协议，提取字段并创建
        let (name, allow_redistribution, allow_modification, restrictions_note, allow_backup) = final_state.to_user_license_fields();
        
        match ctx.data().db().license().create(
            ctx.author().id,
            name,
            allow_redistribution,
            allow_modification,
            restrictions_note,
            allow_backup,
        ).await {
            Ok(license) => {
                let success_embed = LicenseEmbedBuilder::create_license_detail_embed(&license);
                interaction.create_followup(
                    ctx.http(),
                    CreateInteractionResponseFollowup::new()
                        .content("✅ 协议创建成功！")
                        .embed(success_embed)
                        .ephemeral(true),
                ).await?;
            }
            Err(e) => {
                let user_message = UserFriendlyErrorMapper::map_operation_error("create_license", &e);
                let suggestion = UserFriendlyErrorMapper::get_user_suggestion(&e);
                
                let content = if let Some(suggestion) = suggestion {
                    format!("❌ {user_message}\n💡 {suggestion}")
                } else {
                    format!("❌ {user_message}")
                };
                
                interaction.create_followup(
                    ctx.http(),
                    CreateInteractionResponseFollowup::new()
                        .content(content)
                        .ephemeral(true),
                ).await?;
            }
        }
    }
    
    Ok(())
}