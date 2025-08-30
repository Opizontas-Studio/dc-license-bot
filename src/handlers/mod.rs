mod auto_publish;
mod auto_publish_flow;
mod ping;

pub use ping::PingHandler;
use serenity::all::{Channel, ChannelType, Context, FullEvent, ActionRowComponent, CreateInteractionResponse, ModalInteraction, MessageId, EditMessage, ChannelId};
use tracing::warn;

use crate::{commands::Data, error::BotError, utils::{EditorCore, LicenseEditState}};

pub async fn poise_event_handler(
    ctx: &Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, BotError>,
    data: &Data,
) -> Result<(), BotError> {
    match event {
        FullEvent::InteractionCreate { interaction } => {
            // 处理 Modal 提交事件
            if let Some(modal_interaction) = interaction.as_modal_submit() {
                handle_modal_submit(ctx, modal_interaction).await?;
            }
        }
        FullEvent::ThreadCreate { thread } => {
            // 检查是否是论坛类型频道中的线程
            if let Ok(Channel::Guild(guild_channel)) = thread
                .parent_id
                .unwrap_or_default()
                .to_channel(&ctx.http)
                .await
                && guild_channel.kind == ChannelType::Forum {
                    // 检查论坛频道是否在白名单中
                    let cfg = data.cfg().load();
                    let is_allowed = cfg.allowed_forum_channels.is_empty() 
                        || cfg.allowed_forum_channels.contains(&guild_channel.id);
                    
                    if is_allowed {
                        // 处理论坛线程创建事件 - 调用自动发布逻辑
                        tracing::info!("Forum thread created in allowed channel: {}", thread.name());
                        if let Err(e) = auto_publish::handle_thread_create(ctx, thread, data).await {
                            tracing::error!("Auto publish failed: {}", e);
                        }
                    } else {
                        tracing::debug!(
                            "Forum thread created in non-allowed channel '{}' (ID: {}), skipping auto publish",
                            guild_channel.name,
                            guild_channel.id
                        );
                    }
                }
        }
        _ => {}
    }
    Ok(())
}

/// 处理Modal提交事件
async fn handle_modal_submit(
    ctx: &Context,
    modal_interaction: &ModalInteraction,
) -> Result<(), BotError> {
    let custom_id = &modal_interaction.data.custom_id;
    
    // 解析 custom_id 以获取消息ID和操作类型
    if let Some((action, message_id_str)) = parse_modal_custom_id(custom_id) {
        if let Ok(message_id) = message_id_str.parse::<u64>() {
            let message_id = MessageId::new(message_id);
            
            match action {
                "edit_name_modal" => {
                    if let Some(updated_state) = handle_name_edit_modal(ctx, modal_interaction, message_id).await? {
                        update_license_editor_message(ctx, modal_interaction, message_id, updated_state).await?;
                    }
                }
                "edit_restrictions_modal" => {
                    if let Some(updated_state) = handle_restrictions_edit_modal(ctx, modal_interaction, message_id).await? {
                        update_license_editor_message(ctx, modal_interaction, message_id, updated_state).await?;
                    }
                }
                _ => {
                    warn!("Unknown modal action: {}", action);
                    acknowledge_modal(ctx, modal_interaction).await?;
                }
            }
        } else {
            warn!("Failed to parse message ID from custom_id: {}", custom_id);
            acknowledge_modal(ctx, modal_interaction).await?;
        }
    } else {
        warn!("Unknown modal custom_id format: {}", custom_id);
        acknowledge_modal(ctx, modal_interaction).await?;
    }
    
    Ok(())
}

/// 解析 Modal custom_id 格式：action_messageId
fn parse_modal_custom_id(custom_id: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = custom_id.rsplitn(2, '_').collect();
    if parts.len() == 2 {
        Some((parts[1], parts[0]))
    } else {
        None
    }
}

/// 处理协议名称编辑Modal
async fn handle_name_edit_modal(ctx: &Context, modal_interaction: &ModalInteraction, message_id: MessageId) -> Result<Option<LicenseEditState>, BotError> {
    if let Some(ActionRowComponent::InputText(input)) = modal_interaction
        .data
        .components
        .first()
        .and_then(|row| row.components.first())
    {
        let license_name = input.value.clone().unwrap_or_default();
        
        // 确认Modal提交
        acknowledge_modal(ctx, modal_interaction).await?;
        
        // 从现有消息恢复完整状态
        let mut state = recover_editor_state(ctx, modal_interaction.channel_id, message_id).await?;
        
        // 更新名称
        state.license_name = license_name;
        
        tracing::info!("License name updated to: {}", state.license_name);
        Ok(Some(state))
    } else {
        acknowledge_modal(ctx, modal_interaction).await?;
        Ok(None)
    }
}

/// 处理限制条件编辑Modal
async fn handle_restrictions_edit_modal(ctx: &Context, modal_interaction: &ModalInteraction, message_id: MessageId) -> Result<Option<LicenseEditState>, BotError> {
    if let Some(ActionRowComponent::InputText(input)) = modal_interaction
        .data
        .components
        .first()
        .and_then(|row| row.components.first())
    {
        let restrictions = input.value.clone().unwrap_or_default();
        let restrictions_note = if restrictions.trim().is_empty() {
            None
        } else {
            Some(restrictions)
        };
        
        // 确认Modal提交
        acknowledge_modal(ctx, modal_interaction).await?;
        
        // 从现有消息恢复完整状态
        let mut state = recover_editor_state(ctx, modal_interaction.channel_id, message_id).await?;
        
        // 更新限制条件
        state.restrictions_note = restrictions_note.clone();
        
        tracing::info!("License restrictions updated to: {:?}", restrictions_note);
        Ok(Some(state))
    } else {
        acknowledge_modal(ctx, modal_interaction).await?;
        Ok(None)
    }
}

/// 确认Modal提交
async fn acknowledge_modal(ctx: &Context, modal_interaction: &ModalInteraction) -> Result<(), BotError> {
    modal_interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Acknowledge,
        )
        .await?;
    Ok(())
}

/// 更新协议编辑器消息
async fn update_license_editor_message(
    ctx: &Context,
    modal_interaction: &ModalInteraction,
    message_id: MessageId,
    state: LicenseEditState,
) -> Result<(), BotError> {
    // 创建编辑器核心并构建UI
    let editor_core = EditorCore::new(state);
    let (embed, components) = editor_core.build_ui();
    
    // 更新消息
    ctx.http
        .edit_message(
            modal_interaction.channel_id,
            message_id,
            &EditMessage::new()
                .embed(embed)
                .components(components),
            vec![]
        )
        .await?;
    
    Ok(())
}

/// 从现有消息中恢复编辑器状态
/// 暂时使用简化版本，只从embed标题中获取协议名称
async fn recover_editor_state(
    ctx: &Context,
    channel_id: ChannelId,
    message_id: MessageId,
) -> Result<LicenseEditState, BotError> {
    // 获取原始消息
    let message = ctx.http.get_message(channel_id, message_id).await?;
    
    // 从embed中提取协议名称
    let license_name = if let Some(embed) = message.embeds.first() {
        embed.title.clone().unwrap_or_else(|| "未命名协议".to_string())
    } else {
        "未命名协议".to_string()
    };
    
    // 暂时创建基础状态 - TODO: 完整的状态恢复需要解析按钮和embed内容
    let mut state = LicenseEditState::new(license_name);
    
    // 从embed描述中提取一些基础信息
    if let Some(embed) = message.embeds.first() {
        if let Some(description) = &embed.description {
            // 简单检查是否包含已启用的功能标记
            if description.contains("✅ 允许二次传播") {
                state.allow_redistribution = true;
            }
            if description.contains("✅ 允许二次修改") {
                state.allow_modification = true;
            }
            if description.contains("✅ 允许备份") {
                state.allow_backup = true;
            }
            
            // 尝试提取限制条件
            if let Some(restrictions_start) = description.find("**限制条件**") {
                if let Some(restrictions_line) = description[restrictions_start..].lines().nth(1) {
                    let restrictions_text = restrictions_line.trim();
                    if !restrictions_text.is_empty() && restrictions_text != "无" {
                        state.restrictions_note = Some(restrictions_text.to_string());
                    }
                }
            }
        }
    }
    
    Ok(state)
}
