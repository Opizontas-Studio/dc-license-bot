use poise::{CreateReply, command};
use serenity::all::*;
use tracing::{warn, info, error};

use crate::{commands::Context, error::BotError, services::notification_service::NotificationPayload};

#[command(
    slash_command,
    owners_only,
    global_cooldown = 10,
    name_localized("zh-CN", "发布协议"),
    description_localized("zh-CN", "在当前帖子发布协议"),
    ephemeral
)]
/// Publishes the license in the current thread
pub async fn publish_license(
    ctx: Context<'_>,
    #[name_localized("zh-CN", "协议")]
    #[description_localized("zh-CN", "选择要发布的协议")]
    #[autocomplete = "autocomplete_license"]
    license_id: String,

    #[name_localized("zh-CN", "备份权限")]
    #[description_localized("zh-CN", "覆盖协议中的备份权限设置（可选）")]
    backup_override: Option<bool>,
) -> Result<(), BotError> {
    let db = ctx.data().db.clone();

    // 1. 前置安全检查
    // 检查是否在帖子中
    let channel = ctx.channel_id().to_channel(&ctx).await?;
    let is_thread = matches!(
        channel,
        Channel::Guild(GuildChannel {
            kind: ChannelType::PublicThread | ChannelType::PrivateThread | ChannelType::NewsThread,
            ..
        })
    );

    if !is_thread {
        ctx.send(
            CreateReply::default()
                .content("请在您创建的帖子中使用本命令。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    // 获取thread信息
    let thread = channel.guild().unwrap();

    // 检查是否是帖子创建者
    if thread.owner_id != Some(ctx.author().id) {
        ctx.send(
            CreateReply::default()
                .content("您只能为自己创建的帖子添加授权协议。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    // 2. 获取选择的协议
    let license = if let Some(user_id_str) = license_id.strip_prefix("user:") {
        // 用户协议
        let user_id = match user_id_str.parse::<i32>() {
            Ok(id) => id,
            Err(_) => {
                ctx.send(
                    CreateReply::default()
                        .content("无效的协议ID格式。")
                        .ephemeral(true),
                )
                .await?;
                return Ok(());
            }
        };
        let Some(license) = db
            .license()
            .get_license(user_id, ctx.author().id)
            .await?
        else {
            ctx.send(
                CreateReply::default()
                    .content("未找到该协议。")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        };
        license
    } else if let Some(system_name) = license_id.strip_prefix("system:") {
        // 系统协议
        let system_licenses = ctx.data().system_license_cache.get_all().await;
        let Some(system_license) = system_licenses.iter().find(|l| l.license_name == system_name) else {
            ctx.send(
                CreateReply::default()
                    .content("未找到该系统协议。")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        };
        
        // 将系统协议转换为数据库模型格式
        // 使用一个虚拟的ID，因为这是系统协议
        system_license.to_user_license(ctx.author().id, -1)
    } else {
        ctx.send(
            CreateReply::default()
                .content("无效的协议格式。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };

    // 应用备份权限覆盖
    let backup_allowed = backup_override.unwrap_or(license.allow_backup);

    // 3. 生成预览embed
    let display_name = ctx.author_member().await
        .map(|m| m.display_name().to_string())
        .unwrap_or_else(|| ctx.author().name.clone());
    let preview_embed = create_license_embed(&license, backup_allowed, ctx.author(), &display_name);

    // 创建按钮
    let publish_btn = CreateButton::new("publish_license")
        .label("✅ 发布")
        .style(ButtonStyle::Success);
    let cancel_btn = CreateButton::new("cancel_publish")
        .label("❌ 取消")
        .style(ButtonStyle::Danger);

    let reply =
        CreateReply::default()
            .embed(preview_embed)
            .components(vec![CreateActionRow::Buttons(vec![
                publish_btn,
                cancel_btn,
            ])]);

    let handler = ctx.send(reply).await?;

    // 4. 等待用户交互
    let Some(interaction) = handler
        .message()
        .await?
        .await_component_interaction(ctx)
        .author_id(ctx.author().id)
        .await
    else {
        warn!("Interaction timed out");
        return Ok(());
    };

    match interaction.data.custom_id.as_str() {
        "publish_license" => {
            interaction
                .create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;

            // 检查是否已有协议
            let existing_post = db.published_posts().get_by_thread(thread.id).await?;

            if let Some(existing) = existing_post {
                // 编辑旧协议消息为作废
                if let Ok(mut old_msg) = ctx
                    .http()
                    .get_message(thread.id, MessageId::new(existing.message_id as u64))
                    .await
                {
                    // 获取原有的 embed
                    if let Some(original_embed) = old_msg.embeds.first() {
                        let mut updated_embed = CreateEmbed::new()
                            .title(format!("⚠️ [已作废] {}", original_embed.title.as_deref().unwrap_or("授权协议")))
                            .description(format!(
                                "**此协议已被新协议替换**\n\n{}",
                                original_embed.description.as_deref().unwrap_or("")
                            ))
                            .colour(Colour::from_rgb(128, 128, 128)); // 灰色表示已作废
                        
                        // 保留原有的字段
                        for field in &original_embed.fields {
                            updated_embed = updated_embed.field(&field.name, &field.value, field.inline);
                        }
                        
                        // 保留原有的 footer 并添加作废时间
                        if let Some(footer) = &original_embed.footer {
                            updated_embed = updated_embed.footer(CreateEmbedFooter::new(
                                format!("{} | 作废于 {}", 
                                    &footer.text, 
                                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
                                )
                            ));
                        }
                        
                        let _ = old_msg
                            .edit(ctx, EditMessage::new().embed(updated_embed))
                            .await;
                    }

                    // Unpin旧消息
                    let _ = old_msg.unpin(ctx).await;
                }
            }

            // 发布新协议
            let license_embed = create_license_embed(&license, backup_allowed, ctx.author(), &display_name);
            let new_msg = thread
                .send_message(ctx, CreateMessage::new().embed(license_embed))
                .await?;

            // Pin新消息
            let _ = new_msg.pin(ctx).await;

            // 检查备份权限是否变更
            let backup_changed = db.published_posts()
                .has_backup_permission_changed(thread.id, backup_allowed)
                .await?;

            // 更新数据库
            db.published_posts()
                .record_or_update(thread.id, new_msg.id, ctx.author().id, backup_allowed)
                .await?;

            // 如果备份权限发生变更，发送通知
            if backup_changed {
                info!("备份权限发生变更，发送通知");
                
                // 获取帖子首楼消息作为内容预览
                let content_preview = get_thread_first_message_content(ctx, &thread).await
                    .unwrap_or_else(|_| "无法获取内容预览".to_string());
                
                let notification_payload = NotificationPayload::from_discord_context(
                    thread.guild_id,
                    thread.parent_id.unwrap(), // 父频道ID
                    thread.id,                 // 帖子ID
                    new_msg.id,
                    ctx.author().id,
                    ctx.author().name.clone(),
                    display_name.to_string(),
                    thread.name.clone(),
                    content_preview,
                    license.license_name.clone(),
                    backup_allowed,
                ).await;
                
                if let Err(e) = ctx.data().notification_service.send_backup_notification(&notification_payload).await {
                    error!("发送备份通知失败: {}", e);
                }
            }

            // 更新回复
            handler
                .edit(
                    ctx,
                    CreateReply::default()
                        .embed(
                            CreateEmbed::new()
                                .title("✅ 协议已发布")
                                .description(format!(
                                    "协议 '{}' 已成功发布到当前帖子。",
                                    license.license_name
                                ))
                                .colour(Colour::DARK_GREEN),
                        )
                        .components(vec![]),
                )
                .await?;
        }
        "cancel_publish" => {
            interaction
                .create_response(ctx, CreateInteractionResponse::Acknowledge)
                .await?;

            handler
                .edit(
                    ctx,
                    CreateReply::default()
                        .content("已取消发布协议。")
                        .components(vec![]),
                )
                .await?;
        }
        _ => {}
    }

    Ok(())
}

// 自动补全函数
async fn autocomplete_license(
    ctx: Context<'_>,
    partial: &str,
) -> impl Iterator<Item = poise::serenity_prelude::AutocompleteChoice> {
    let db = ctx.data().db.clone();

    // 获取用户的个人协议
    let user_licenses = match db.license().get_user_licenses(ctx.author().id).await {
        Ok(licenses) => licenses,
        Err(_) => vec![],
    };

    let system_licenses = ctx.data().system_license_cache.get_all().await;

    // 组合并过滤
    user_licenses
        .into_iter()
        .map(|l| {
            let name = l.license_name.clone();
            let value = format!("user:{}", l.id);
            (name, value)
        })
        .chain(
            system_licenses
                .into_iter()
                .map(|l| {
                    let display_name = format!("{} (系统)", l.license_name);
                    let value = format!("system:{}", l.license_name);
                    (display_name, value)
                }),
        )
        .filter(|(name, _)| name.to_lowercase().contains(&partial.to_lowercase()))
        .take(25)
        .map(|(name, value)| poise::serenity_prelude::AutocompleteChoice::new(name, value))
        .into_iter()
}

// 创建协议embed
fn create_license_embed(
    license: &entities::entities::user_licenses::Model,
    backup_allowed: bool,
    author: &User,
    display_name: &str,
) -> CreateEmbed {
    CreateEmbed::new()
        .title(format!("📜 授权协议: {}", license.license_name))
        .description("本帖子内容受以下授权协议保护：")
        .field(
            "允许二次传播",
            if license.allow_redistribution {
                "✅ 允许"
            } else {
                "❌ 不允许"
            },
            true,
        )
        .field(
            "允许二次修改",
            if license.allow_modification {
                "✅ 允许"
            } else {
                "❌ 不允许"
            },
            true,
        )
        .field(
            "允许备份",
            if backup_allowed {
                "✅ 允许"
            } else {
                "❌ 不允许"
            },
            true,
        )
        .field(
            "限制条件",
            license.restrictions_note.as_deref().unwrap_or("无特殊限制"),
            false,
        )
        .footer(CreateEmbedFooter::new(format!("发布者: {}", display_name)))
        .timestamp(serenity::model::Timestamp::now())
        .colour(Colour::BLUE)
}

// 获取帖子首楼消息内容
async fn get_thread_first_message_content(
    ctx: Context<'_>,
    thread: &GuildChannel,
) -> Result<String, BotError> {
    // 尝试获取帖子的首楼消息
    // 通常帖子的首楼消息ID就是帖子ID本身
    let first_message = ctx.http().get_message(thread.id, MessageId::new(thread.id.get())).await?;
    
    if !first_message.author.bot && !first_message.content.is_empty() {
        Ok(first_message.content)
    } else {
        Ok("该帖子暂无文本内容".to_string())
    }
}
