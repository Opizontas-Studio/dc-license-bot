use serenity::all::{
    ButtonStyle, ChannelId, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter, 
    CreateMessage, GuildChannel, Timestamp, UserId, Colour, Context, CreateInteractionResponse,
    CreateInteractionResponseMessage
};
use crate::{
    commands::Data,
    error::BotError,
    types::license::DefaultLicenseIdentifier,
    services::license::LicensePublishService,
};

pub async fn handle_thread_create(
    ctx: &Context,
    thread: &GuildChannel,
    data: &Data,
) -> Result<(), BotError> {
    // 1. 获取帖子创建者
    let Some(owner_id) = thread.owner_id else { return Ok(()); };

    // 2. 检查用户是否启用了自动发布
    if !data.db().user_settings().is_auto_publish_enabled(owner_id).await? {
        return Ok(()); // 用户未启用，静默退出
    }

    // 3. 获取用户的默认协议
    let Some(default_license_id) = data.db().user_settings().get_default_license(owner_id).await? else {
        // 用户启用了功能但未设置默认协议，静默退出
        return Ok(());
    };

    // 4. 根据协议ID获取完整的协议内容 (User 或 System)
    let license_model = match default_license_id {
        DefaultLicenseIdentifier::User(id) => {
            let Some(license) = data.db().license().get_license(id, owner_id).await? else {
                return Ok(()); // 协议不存在，静默退出
            };
            license
        }
        DefaultLicenseIdentifier::System(name) => {
            let Some(sys_license) = data.system_license_cache().get_all().await.into_iter()
                .find(|l| l.license_name == name) else {
                return Ok(()); // 系统协议不存在，静默退出
            };
            sys_license.to_user_license(owner_id, -1)
        }
    };

    // 5. 构建交互式面板 (Embed + 确认/取消按钮)
    let embed = create_license_preview_embed(&license_model, owner_id).await?;
    
    let confirm_btn = CreateButton::new("confirm_auto_publish")
        .label("✅ 确认发布")
        .style(ButtonStyle::Success);
    
    let cancel_btn = CreateButton::new("cancel_auto_publish")
        .label("❌ 取消")
        .style(ButtonStyle::Danger);

    let action_row = CreateActionRow::Buttons(vec![confirm_btn, cancel_btn]);

    // 6. 在新帖子中发送面板
    let message = CreateMessage::new()
        .embed(embed)
        .components(vec![action_row]);

    let sent_message = ChannelId::new(thread.id.get())
        .send_message(&ctx.http, message)
        .await?;

    // 7. 等待并处理面板交互
    let Some(interaction) = sent_message
        .await_component_interaction(&ctx.shard)
        .author_id(owner_id)
        .timeout(std::time::Duration::from_secs(300)) // 5分钟超时
        .await else {
        // 超时，删除消息
        let _ = sent_message.delete(&ctx.http).await;
        return Ok(());
    };

    match interaction.data.custom_id.as_str() {
        "confirm_auto_publish" => {
            // 确认发布 - 使用统一的发布服务
            let display_name = ctx.cache
                .member(thread.guild_id, owner_id)
                .map(|m| m.display_name().to_string())
                .unwrap_or_else(|| format!("<@{}>", owner_id.get()));
            
            LicensePublishService::publish(
                &ctx.http,
                data,
                thread,
                &license_model,
                license_model.allow_backup, // 自动发布使用协议本身的备份设置
                owner_id,
                &format!("User_{}", owner_id.get()), // 简化的用户名，因为我们有display_name
                &display_name,
            ).await?;
            
            // 删除交互面板
            let _ = sent_message.delete(&ctx.http).await;
            
            // 回应交互
            interaction.create_response(&ctx.http, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("✅ 协议已成功发布！")
                    .ephemeral(true)
            )).await?;
        }
        "cancel_auto_publish" => {
            // 取消发布 - 删除面板
            let _ = sent_message.delete(&ctx.http).await;
            
            // 回应交互
            interaction.create_response(&ctx.http, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("❌ 已取消发布")
                    .ephemeral(true)
            )).await?;
        }
        _ => {}
    }

    Ok(())
}

async fn create_license_preview_embed(
    license: &entities::user_licenses::Model,
    owner_id: UserId,
) -> Result<CreateEmbed, BotError> {
    Ok(CreateEmbed::new()
        .title(format!("📜 准备发布协议: {}", license.license_name))
        .description("检测到您启用了自动发布功能，是否要为此帖子发布以下协议？")
        .field("允许二次传播", 
               if license.allow_redistribution { "✅ 允许" } else { "❌ 不允许" }, true)
        .field("允许二次修改", 
               if license.allow_modification { "✅ 允许" } else { "❌ 不允许" }, true)
        .field("允许备份", 
               if license.allow_backup { "✅ 允许" } else { "❌ 不允许" }, true)
        .field("限制条件", 
               license.restrictions_note.as_deref().unwrap_or("无特殊限制"), false)
        .footer(CreateEmbedFooter::new(format!("作者: <@{}>", owner_id.get())))
        .timestamp(Timestamp::now())
        .colour(Colour::GOLD))
}

