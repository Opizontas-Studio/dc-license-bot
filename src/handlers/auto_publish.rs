use std::{collections::HashMap, sync::OnceLock, time::Instant};

use serenity::all::{
    ButtonStyle, ChannelId, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
    GuildChannel, UserId, CreateSelectMenu, CreateSelectMenuOption, Context,
    ComponentInteractionDataKind, CreateSelectMenuKind, CreateInteractionResponseFollowup,
};
use tokio::sync::RwLock;

use crate::{
    commands::Data, error::BotError, services::license::LicensePublishService,
    types::license::DefaultLicenseIdentifier, 
    utils::{LicenseEmbedBuilder, LicenseEditState, present_license_editing_panel_with_serenity_context},
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
                    let mut license = sys_license.to_user_license(owner_id, -1);
                    // 如果用户设置了系统协议的备份权限覆盖，使用用户的设置
                    if let Some(backup_override) = settings.default_system_license_backup {
                        license.allow_backup = backup_override;
                    }
                    license
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
            if let Err(e) = handle_license_selection_flow(ctx, thread, &interaction, data, owner_id).await {
                tracing::error!("协议选择流程失败: {}", e);
                // 发送错误消息
                interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("❌ 协议设置过程中发生错误，请稍后重试。")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
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

/// 处理协议选择和编辑流程
async fn handle_license_selection_flow(
    ctx: &Context,
    thread: &GuildChannel,
    interaction: &serenity::all::ComponentInteraction,
    data: &Data,
    owner_id: UserId,
) -> Result<(), BotError> {
    // 1. 获取所有可用的系统协议
    let system_licenses = data.system_license_cache().get_all().await;
    
    // 2. 创建选择菜单
    let mut select_options = vec![
        CreateSelectMenuOption::new("创建新协议", "new_license")
            .description("创建一个全新的协议")
    ];
    
    // 添加系统协议选项
    for license in &system_licenses {
        select_options.push(
            CreateSelectMenuOption::new(
                &license.license_name,
                format!("system_{}", license.license_name)
            )
            .description("基于系统协议创建")
        );
    }
    
    let select_menu = CreateSelectMenu::new("license_selection", CreateSelectMenuKind::String { options: select_options })
        .placeholder("请选择协议类型")
        .max_values(1);
    
    // 3. 发送选择菜单
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("请选择你要使用的协议：")
                    .components(vec![CreateActionRow::SelectMenu(select_menu)])
                    .ephemeral(true),
            ),
        )
        .await?;
    
    // 4. 等待用户选择
    let Some(select_interaction) = interaction
        .get_response(&ctx.http)
        .await?
        .await_component_interaction(&ctx.shard)
        .author_id(owner_id)
        .timeout(std::time::Duration::from_secs(120))
        .await
    else {
        return Ok(()); // 超时，静默退出
    };
    
    // 5. 处理用户选择
    if let ComponentInteractionDataKind::StringSelect { values } = &select_interaction.data.kind {
        if let Some(selected) = values.first() {
            let initial_state = if selected == "new_license" {
                // 创建新协议
                LicenseEditState::new("新协议".to_string())
            } else if let Some(system_name) = selected.strip_prefix("system_") {
                // 基于系统协议创建
                if let Some(system_license) = system_licenses.iter().find(|l| l.license_name == system_name) {
                    LicenseEditState::from_system_license(system_license)
                } else {
                    return Err(BotError::GenericError {
                        message: "选择的系统协议不存在".to_string(),
                        source: None,
                    });
                }
            } else {
                return Err(BotError::GenericError {
                    message: "无效的选择".to_string(),
                    source: None,
                });
            };
            
            // 6. 调用完整的协议编辑流程
            match present_license_editing_panel_with_serenity_context(ctx, data, &select_interaction, initial_state).await {
                Ok(Some(final_state)) => {
                    // 用户保存了协议，创建并设置为默认
                    match save_license_and_set_default(data, owner_id, final_state).await {
                        Ok(license) => {
                            // 成功保存，启动发布确认流程
                            if let Err(e) = handle_publish_confirmation(ctx, data, thread, &select_interaction, owner_id, license).await {
                                tracing::error!("发布确认流程失败: {}", e);
                                // 发送错误消息
                                select_interaction
                                    .create_followup(
                                        &ctx.http,
                                        CreateInteractionResponseFollowup::new()
                                            .content("❌ 协议已保存，但发布确认过程中发生错误。")
                                            .ephemeral(true),
                                    )
                                    .await?;
                            }
                        }
                        Err(e) => {
                            tracing::error!("保存协议失败: {}", e);
                            // 由于编辑器已经清理了UI，我们需要通过followup发送错误消息
                            select_interaction
                                .create_followup(
                                    &ctx.http,
                                    CreateInteractionResponseFollowup::new()
                                        .content("❌ 协议保存失败，请稍后重试。")
                                        .ephemeral(true),
                                )
                                .await?;
                        }
                    }
                }
                Ok(None) => {
                    // 用户取消了编辑
                    select_interaction
                        .create_followup(
                            &ctx.http,
                            CreateInteractionResponseFollowup::new()
                                .content("已取消协议创建。自动发布功能已启用，但您需要手动设置默认协议。")
                                .ephemeral(true),
                        )
                        .await?;
                }
                Err(e) => {
                    tracing::error!("协议编辑流程失败: {}", e);
                    select_interaction
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content("❌ 协议编辑过程中发生错误，请稍后重试。")
                                    .ephemeral(true),
                            ),
                        )
                        .await?;
                }
            }
        }
    }
    
    Ok(())
}

/// 处理发布确认流程
async fn handle_publish_confirmation(
    ctx: &Context,
    data: &Data,
    thread: &GuildChannel,
    interaction: &serenity::all::ComponentInteraction,
    owner_id: UserId,
    license: crate::services::license::UserLicense,
) -> Result<(), BotError> {
    // 发送确认消息和按钮
    let confirm_message = format!(
        "✅ 协议「{}」已创建并设置为默认协议！\n\n是否要在当前帖子中发布此协议？",
        license.license_name
    );
    
    let publish_btn = CreateButton::new("confirm_publish_new_license")
        .label("是的，发布")
        .style(ButtonStyle::Success);
    
    let skip_btn = CreateButton::new("skip_publish_new_license")
        .label("暂不发布")
        .style(ButtonStyle::Secondary);
    
    let action_row = CreateActionRow::Buttons(vec![publish_btn, skip_btn]);
    
    // 发送确认消息
    interaction
        .create_followup(
            &ctx.http,
            CreateInteractionResponseFollowup::new()
                .content(confirm_message)
                .components(vec![action_row])
                .ephemeral(true),
        )
        .await?;
    
    // 等待用户交互
    let Some(publish_interaction) = interaction
        .get_response(&ctx.http)
        .await?
        .await_component_interaction(&ctx.shard)
        .author_id(owner_id)
        .timeout(std::time::Duration::from_secs(120)) // 2分钟超时
        .await
    else {
        // 超时，删除确认消息并发送新消息
        let response = interaction.get_response(&ctx.http).await?;
        let _ = interaction.delete_followup(&ctx.http, response.id).await;
        
        interaction
            .create_followup(
                &ctx.http,
                CreateInteractionResponseFollowup::new()
                    .content("协议已创建并设置为默认协议！自动发布功能现在已完全启用。")
                    .ephemeral(true),
            )
            .await?;
        return Ok(());
    };
    
    match publish_interaction.data.custom_id.as_str() {
        "confirm_publish_new_license" => {
            // 用户选择发布
            let user = owner_id.to_user(ctx).await?;
            
            // 调用发布服务
            LicensePublishService::publish(
                &ctx.http,
                data,
                thread,
                &license,
                license.allow_backup,
                user,
            )
            .await?;
            
            // 确认发布成功
            publish_interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("✅ 协议已成功发布到当前帖子！")
                            .ephemeral(true),
                    ),
                )
                .await?;
                
            // 清理确认消息
            let response = interaction.get_response(&ctx.http).await?;
            let _ = interaction.delete_followup(&ctx.http, response.id).await;
            
            interaction
                .create_followup(
                    &ctx.http,
                    CreateInteractionResponseFollowup::new()
                        .content("协议已创建、设置为默认协议，并发布到当前帖子！")
                        .ephemeral(true),
                )
                .await?;
        }
        "skip_publish_new_license" => {
            // 用户选择不发布
            publish_interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("好的，协议已保存。你可以稍后手动发布或在新帖子中自动发布。")
                            .ephemeral(true),
                    ),
                )
                .await?;
                
            // 清理确认消息
            let response = interaction.get_response(&ctx.http).await?;
            let _ = interaction.delete_followup(&ctx.http, response.id).await;
            
            interaction
                .create_followup(
                    &ctx.http,
                    CreateInteractionResponseFollowup::new()
                        .content("协议已创建并设置为默认协议！自动发布功能现在已完全启用。")
                        .ephemeral(true),
                )
                .await?;
        }
        _ => {}
    }
    
    Ok(())
}

/// 保存协议并设置为默认协议，返回创建的协议
async fn save_license_and_set_default(
    data: &Data,
    owner_id: UserId,
    final_state: LicenseEditState,
) -> Result<crate::services::license::UserLicense, BotError> {
    // 提取协议字段
    let (name, allow_redistribution, allow_modification, restrictions_note, allow_backup) = 
        final_state.to_user_license_fields();
    
    // 检查用户协议数量是否超过上限
    let current_count = data.db().license().get_user_license_count(owner_id).await?;
    if current_count >= 5 {
        return Err(BotError::GenericError {
            message: "您最多只能创建5个协议，请先删除一些协议。".to_string(),
            source: None,
        });
    }
    
    // 创建协议
    let license = data.db().license().create(
        owner_id,
        name,
        allow_redistribution,
        allow_modification,
        restrictions_note,
        allow_backup,
    ).await?;
    
    // 设置为默认协议
    data.db().user_settings().set_default_license(
        owner_id,
        Some(DefaultLicenseIdentifier::User(license.id)),
        None,
    ).await?;
    
    Ok(license)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::BotDatabase;
    use migration::{Migrator, MigratorTrait, SchemaManager};
    use serenity::all::UserId;
    use crate::utils::LicenseEditState;

    async fn setup_test_db() -> BotDatabase {
        let db = BotDatabase::new_memory().await.unwrap();
        let migrations = Migrator::migrations();
        let manager = SchemaManager::new(db.inner());
        for migration in migrations {
            migration.up(&manager).await.unwrap();
        }
        db
    }

    #[tokio::test]
    async fn test_save_license_and_set_default() {
        let db = setup_test_db().await;
        let user_id = UserId::new(123);
        
        // 创建一个测试的编辑状态
        let edit_state = LicenseEditState::new("Test License".to_string());
        
        // 测试保存协议 - 直接测试数据库层面的逻辑
        let (name, allow_redistribution, allow_modification, restrictions_note, allow_backup) = 
            edit_state.to_user_license_fields();
        
        // 创建协议
        let license = db.license().create(
            user_id,
            name,
            allow_redistribution,
            allow_modification,
            restrictions_note,
            allow_backup,
        ).await.unwrap();
        
        assert_eq!(license.license_name, "Test License");
        assert_eq!(license.user_id, user_id.get() as i64);
        
        // 设置为默认协议
        db.user_settings().set_default_license(
            user_id,
            Some(DefaultLicenseIdentifier::User(license.id)),
            None,
        ).await.unwrap();
        
        // 验证协议已设置为默认
        let settings = db.user_settings().get(user_id).await.unwrap().unwrap();
        assert_eq!(settings.default_user_license_id, Some(license.id));
    }

    #[tokio::test]
    async fn test_save_license_exceeds_limit() {
        let db = setup_test_db().await;
        let user_id = UserId::new(456);
        
        // 先创建5个协议（达到上限）
        for i in 0..5 {
            db.license().create(
                user_id,
                format!("License {}", i),
                false,
                false,
                None,
                false,
            ).await.unwrap();
        }
        
        // 验证协议数量已达到上限
        let count = db.license().get_user_license_count(user_id).await.unwrap();
        assert_eq!(count, 5);
        
        // 尝试创建第6个协议，应该失败
        let result = db.license().create(
            user_id,
            "License 6".to_string(),
            false,
            false,
            None,
            false,
        ).await;
        
        // 在实际代码中，这个检查是在 save_license_and_set_default 中进行的
        // 但这里我们测试的是数据库层面的行为
        assert!(result.is_ok()); // 数据库层面不会阻止，检查是在上层进行的
    }

    #[tokio::test]
    async fn test_license_edit_state_conversion() {
        let edit_state = LicenseEditState::from_existing(
            "Test License".to_string(),
            true,
            false,
            Some("No commercial use".to_string()),
            true,
        );
        
        let (name, allow_redistribution, allow_modification, restrictions_note, allow_backup) = 
            edit_state.to_user_license_fields();
        
        assert_eq!(name, "Test License");
        assert!(allow_redistribution);
        assert!(!allow_modification);
        assert_eq!(restrictions_note, Some("No commercial use".to_string()));
        assert!(allow_backup);
    }
}
