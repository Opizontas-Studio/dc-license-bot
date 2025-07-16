use std::{collections::HashMap, sync::OnceLock, time::Instant};

use serenity::all::{
    ButtonStyle, ChannelId, ComponentInteractionDataKind, Context, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
    GuildChannel, UserId,
};
use tokio::sync::RwLock;

use crate::{
    commands::Data, error::BotError, services::license::LicensePublishService,
    types::license::DefaultLicenseIdentifier, 
    utils::{LicenseEmbedBuilder, LicenseSelectMenuBuilder},
};

// 线程创建事件去重缓存，存储最近处理过的线程ID和处理时间
static PROCESSED_THREADS: OnceLock<RwLock<HashMap<u64, Instant>>> = OnceLock::new();

pub async fn handle_thread_create(
    ctx: &Context,
    thread: &GuildChannel,
    data: &Data,
) -> Result<(), BotError> {
    // 0. 去重检查 - 防止Discord事件重复触发
    let thread_id = thread.id.get();
    let now = Instant::now();

    {
        let cache = PROCESSED_THREADS.get_or_init(|| RwLock::new(HashMap::new()));
        let mut write_cache = cache.write().await;

        // 检查是否已处理过（5分钟内）
        if let Some(&processed_time) = write_cache.get(&thread_id) {
            if now.duration_since(processed_time).as_secs() < 300 {
                tracing::debug!(
                    "Thread {} already processed, skipping duplicate event",
                    thread_id
                );
                return Ok(());
            }
        }

        // 清理过期记录并标记当前线程
        write_cache.retain(|_, &mut time| now.duration_since(time).as_secs() < 300);
        write_cache.insert(thread_id, now);
    }

    // 1. 获取帖子创建者
    let Some(owner_id) = thread.owner_id else {
        return Ok(());
    };

    // 2. 检查用户设置状态
    let user_settings = data.db().user_settings().get(owner_id).await?;

    match user_settings {
        // 场景一：新用户
        None => {
            return handle_new_user_guidance(ctx, thread, data, owner_id).await;
        }
        // 用户已存在
        Some(settings) => {
            if !settings.auto_publish_enabled {
                // 场景三：已关闭功能的用户，静默退出
                return Ok(());
            }
            
            // 场景二：已启用功能的用户
            let default_license_id = if let Some(user_license_id) = settings.default_user_license_id {
                DefaultLicenseIdentifier::User(user_license_id)
            } else if let Some(system_license_name) = settings.default_system_license_name {
                DefaultLicenseIdentifier::System(system_license_name)
            } else {
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
                    let Some(sys_license) = data
                        .system_license_cache()
                        .get_all()
                        .await
                        .into_iter()
                        .find(|l| l.license_name == name)
                    else {
                        return Ok(()); // 系统协议不存在，静默退出
                    };
                    sys_license.to_user_license(owner_id, -1)
                }
            };

            // 5. 检查是否跳过确认
            if settings.skip_auto_publish_confirmation {
                // 直接发布协议
                LicensePublishService::publish(
                    &ctx.http,
                    data,
                    thread,
                    &license_model,
                    license_model.allow_backup, // 自动发布使用协议本身的备份设置
                    owner_id.to_user(ctx).await?,
                )
                .await?;
            } else {
                // 显示确认面板
                let display_name = thread
                    .guild_id
                    .member(&ctx.http, owner_id)
                    .await
                    .map(|m| m.display_name().to_string())?;

                let embed = LicenseEmbedBuilder::create_auto_publish_preview_embed(&license_model, &display_name);

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
                    .timeout(std::time::Duration::from_secs(180)) // 3分钟超时
                    .await
                else {
                    // 超时，删除消息
                    let _ = sent_message.delete(&ctx.http).await;
                    return Ok(());
                };

                match interaction.data.custom_id.as_str() {
                    "confirm_auto_publish" => {
                        // 确认发布 - 使用统一的发布服务
                        LicensePublishService::publish(
                            &ctx.http,
                            data,
                            thread,
                            &license_model,
                            license_model.allow_backup, // 自动发布使用协议本身的备份设置
                            owner_id.to_user(ctx).await?,
                        )
                        .await?;

                        // 删除交互面板
                        let _ = sent_message.delete(&ctx.http).await;

                        // 回应交互
                        interaction
                            .create_response(
                                &ctx.http,
                                CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .content("✅ 协议已成功发布！")
                                        .ephemeral(true),
                                ),
                            )
                            .await?;
                    }
                    "cancel_auto_publish" => {
                        // 取消发布 - 删除面板
                        let _ = sent_message.delete(&ctx.http).await;

                        // 回应交互
                        interaction
                            .create_response(
                                &ctx.http,
                                CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .content("❌ 已取消发布")
                                        .ephemeral(true),
                                ),
                            )
                            .await?;
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

/// 处理新用户引导流程
async fn handle_new_user_guidance(
    ctx: &Context,
    thread: &GuildChannel,
    data: &Data,
    owner_id: UserId,
) -> Result<(), BotError> {
    // 1. 构建引导消息和按钮
    let welcome_message = "你好！我们发现你发了一个新帖子。你是否想开启'自动添加许可协议'的功能呢？";

    let enable_btn = CreateButton::new("enable_auto_publish_setup")
        .label("启用")
        .style(ButtonStyle::Success);

    let disable_btn = CreateButton::new("disable_auto_publish_setup")
        .label("关闭")
        .style(ButtonStyle::Danger);

    let action_row = CreateActionRow::Buttons(vec![enable_btn, disable_btn]);

    let message = CreateMessage::new()
        .content(welcome_message)
        .components(vec![action_row]);

    let sent_message = ChannelId::new(thread.id.get())
        .send_message(&ctx.http, message)
        .await?;

    // 2. 等待并处理交互
    let Some(interaction) = sent_message
        .await_component_interaction(&ctx.shard)
        .author_id(owner_id)
        .timeout(std::time::Duration::from_secs(180)) // 3分钟超时
        .await
    else {
        // 超时，删除消息
        let _ = sent_message.delete(&ctx.http).await;
        return Ok(());
    };

    match interaction.data.custom_id.as_str() {
        "enable_auto_publish_setup" => {
            // 用户选择启用功能
            // 首先将用户状态设置为"已启用"
            data.db().user_settings().set_auto_publish(owner_id, true).await?;
            
            // 启动协议选择流程
            let (user_licenses, system_licenses) = LicenseSelectMenuBuilder::get_all_licenses(data, owner_id.get()).await?;
            
            if user_licenses.is_empty() && system_licenses.is_empty() {
                // 用户没有协议可选
                interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("❌ 你还没有创建任何协议，也没有可用的系统协议。请先使用 `/许可协议` 命令创建协议。")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
            } else {
                // 显示协议选择菜单
                let select_menu = LicenseSelectMenuBuilder::create_license_select_menu(
                    "new_user_select_default_license",
                    "选择你的默认协议",
                    true,  // 包含用户协议
                    true,  // 包含系统协议
                    &user_licenses,
                    &system_licenses,
                );

                interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("请选择你的默认协议：")
                                .components(vec![CreateActionRow::SelectMenu(select_menu)])
                                .ephemeral(true),
                        ),
                    )
                    .await?;

                // 等待用户选择协议
                let Some(select_interaction) = interaction
                    .get_response(&ctx.http)
                    .await?
                    .await_component_interaction(&ctx.shard)
                    .author_id(owner_id)
                    .timeout(std::time::Duration::from_secs(120)) // 2分钟超时
                    .await
                else {
                    // 超时，但用户已经启用了自动发布，只是没有设置默认协议
                    return Ok(());
                };

                if let ComponentInteractionDataKind::StringSelect { values } = &select_interaction.data.kind {
                    if let Some(selected) = values.first() {
                        // 解析选择的协议
                        let license = match LicenseSelectMenuBuilder::parse_selection_value(selected)? {
                            (true, id) => id
                                .parse::<i32>()
                                .ok()
                                .map(DefaultLicenseIdentifier::User),
                            (false, name) => Some(DefaultLicenseIdentifier::System(name)),
                        };

                        // 保存默认协议设置
                        data.db().user_settings().set_default_license(owner_id, license).await?;

                        // 确认消息
                        select_interaction
                            .create_response(
                                &ctx.http,
                                CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .content("✅ 已成功启用自动发布功能并设置默认协议！")
                                        .ephemeral(true),
                                ),
                            )
                            .await?;
                    }
                }
            }
            
            // 删除最初的引导消息
            let _ = sent_message.delete(&ctx.http).await;
        }
        "disable_auto_publish_setup" => {
            // 用户选择关闭功能
            data.db().user_settings().set_auto_publish(owner_id, false).await?;
            
            // 礼貌的回复
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("好的，如果你改变主意，可以随时使用 `/自动发布设置` 手动开启。")
                            .ephemeral(true),
                    ),
                )
                .await?;
            
            // 删除最初的引导消息
            let _ = sent_message.delete(&ctx.http).await;
        }
        _ => {}
    }

    Ok(())
}
