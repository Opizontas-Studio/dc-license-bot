use serenity::all::{
    ButtonStyle, ChannelId, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
    GuildChannel, UserId, CreateSelectMenu, CreateSelectMenuOption, Context,
    ComponentInteractionDataKind, CreateSelectMenuKind, CreateInteractionResponseFollowup,
    Message,
};

use crate::{
    commands::Data, 
    error::BotError, 
    services::license::LicensePublishService,
    types::license::DefaultLicenseIdentifier,
    utils::{LicenseEmbedBuilder, LicenseEditState, present_license_editing_panel},
};

/// 自动发布流程的状态定义
#[derive(Debug, Clone)]
pub enum FlowState {
    /// 初始状态 - 检查用户设置并决定后续流程
    Initial,
    /// 等待新用户选择启用/禁用功能
    AwaitingGuidance,
    /// 选择协议类型（新建或基于系统协议）
    SelectingLicense,
    /// 编辑协议状态，包含当前编辑的协议数据
    EditingLicense(LicenseEditState),
    /// 确认保存协议状态，包含待保存的协议数据
    ConfirmingSave(crate::services::license::UserLicense),
    /// 确认发布协议状态，包含待发布的协议数据
    ConfirmingPublish(crate::services::license::UserLicense),
    /// 完成状态 - 流程结束
    Done,
}

/// 自动发布流程状态机
pub struct AutoPublishFlow<'a> {
    /// 当前状态
    state: FlowState,
    /// Discord 上下文
    ctx: &'a Context,
    /// 应用程序数据
    data: &'a Data,
    /// 帖子所有者ID
    owner_id: UserId,
    /// 当前线程
    thread: &'a GuildChannel,
    /// 当前消息（用于UI更新）
    current_message: Option<Message>,
    /// 缓存的系统协议列表
    system_licenses: Option<Vec<crate::types::license::SystemLicense>>,
}

impl<'a> AutoPublishFlow<'a> {
    /// 创建新的自动发布流程实例
    pub fn new(
        ctx: &'a Context,
        data: &'a Data,
        owner_id: UserId,
        thread: &'a GuildChannel,
    ) -> Self {
        Self {
            state: FlowState::Initial,
            ctx,
            data,
            owner_id,
            thread,
            current_message: None,
            system_licenses: None,
        }
    }

    /// 运行状态机主循环
    pub async fn run(mut self) -> Result<(), BotError> {
        loop {
            match self.handle_state().await {
                Ok(should_continue) => {
                    if !should_continue {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("状态机处理错误: {}", e);
                    // 清理资源并退出
                    self.cleanup().await;
                    return Err(e);
                }
            }
        }
        
        // 正常完成，清理资源
        self.cleanup().await;
        Ok(())
    }

    /// 处理当前状态的逻辑
    /// 返回 Ok(true) 表示继续执行，Ok(false) 表示完成
    async fn handle_state(&mut self) -> Result<bool, BotError> {
        match &self.state {
            FlowState::Initial => self.handle_initial_state().await,
            FlowState::AwaitingGuidance => self.handle_awaiting_guidance_state().await,
            FlowState::SelectingLicense => self.handle_selecting_license_state().await,
            FlowState::EditingLicense(edit_state) => {
                let edit_state = edit_state.clone();
                self.handle_editing_license_state(edit_state).await
            }
            FlowState::ConfirmingSave(license) => {
                let license = license.clone();
                self.handle_confirming_save_state(license).await
            }
            FlowState::ConfirmingPublish(license) => {
                let license = license.clone();
                self.handle_confirming_publish_state(license).await
            }
            FlowState::Done => Ok(false), // 完成状态，退出循环
        }
    }

    /// 转换到新状态
    fn transition_to(&mut self, new_state: FlowState) {
        tracing::debug!("状态转换: {:?} -> {:?}", self.state, new_state);
        self.state = new_state;
    }

    /// 清理资源
    async fn cleanup(&mut self) {
        if let Some(message) = &self.current_message {
            let _ = message.delete(&self.ctx.http).await;
        }
    }

    /// 处理初始状态 - 检查用户设置并决定后续流程
    async fn handle_initial_state(&mut self) -> Result<bool, BotError> {
        // 检查用户设置状态
        let user_settings = self.data.db().user_settings().get(self.owner_id).await?;

        match user_settings {
            // 场景一：新用户
            None => {
                self.transition_to(FlowState::AwaitingGuidance);
                Ok(true)
            }
            // 用户已存在
            Some(settings) => {
                if !settings.auto_publish_enabled {
                    // 场景三：已关闭功能的用户，静默退出
                    self.transition_to(FlowState::Done);
                    return Ok(true);
                }
                
                // 场景二：已启用功能的用户
                let default_license_id = if let Some(user_license_id) = settings.default_user_license_id {
                    DefaultLicenseIdentifier::User(user_license_id)
                } else if let Some(ref system_license_name) = settings.default_system_license_name {
                    DefaultLicenseIdentifier::System(system_license_name.clone())
                } else {
                    // 用户启用了功能但未设置默认协议，静默退出
                    self.transition_to(FlowState::Done);
                    return Ok(true);
                };

                // 根据协议ID获取完整的协议内容
                let license_model = self.get_license_model(&default_license_id, &settings).await?;
                
                if let Some(license) = license_model {
                    // 检查是否跳过确认
                    if settings.skip_auto_publish_confirmation {
                        // 直接发布协议
                        self.publish_license_directly(&license).await?;
                        self.transition_to(FlowState::Done);
                        Ok(true)
                    } else {
                        // 显示确认面板
                        self.show_auto_publish_confirmation(&license).await?;
                        self.transition_to(FlowState::ConfirmingPublish(license));
                        Ok(true)
                    }
                } else {
                    // 协议不存在，静默退出
                    self.transition_to(FlowState::Done);
                    Ok(true)
                }
            }
        }
    }

    /// 获取协议模型
    async fn get_license_model(
        &self,
        license_id: &DefaultLicenseIdentifier,
        settings: &entities::entities::user_settings::Model,
    ) -> Result<Option<crate::services::license::UserLicense>, BotError> {
        match license_id {
            DefaultLicenseIdentifier::User(id) => {
                Ok(self.data.db().license().get_license(*id, self.owner_id).await?)
            }
            DefaultLicenseIdentifier::System(name) => {
                let Some(sys_license) = self
                    .data
                    .system_license_cache()
                    .get_all()
                    .await
                    .into_iter()
                    .find(|l| l.license_name == *name)
                else {
                    return Ok(None);
                };
                
                let mut license = sys_license.to_user_license(self.owner_id, -1);
                // 如果用户设置了系统协议的备份权限覆盖，使用用户的设置
                if let Some(backup_override) = settings.default_system_license_backup {
                    license.allow_backup = backup_override;
                }
                Ok(Some(license))
            }
        }
    }

    /// 直接发布协议
    async fn publish_license_directly(&self, license: &crate::services::license::UserLicense) -> Result<(), BotError> {
        LicensePublishService::publish(
            &self.ctx.http,
            self.data,
            self.thread,
            license,
            license.allow_backup,
            self.owner_id.to_user(self.ctx).await?,
        )
        .await
    }

    /// 显示自动发布确认面板
    async fn show_auto_publish_confirmation(&mut self, license: &crate::services::license::UserLicense) -> Result<(), BotError> {
        let display_name = self.thread
            .guild_id
            .member(&self.ctx.http, self.owner_id)
            .await
            .map(|m| m.display_name().to_string())?;

        let embed = LicenseEmbedBuilder::create_auto_publish_preview_embed(license, &display_name);

        let confirm_btn = CreateButton::new("confirm_auto_publish")
            .label("✅ 确认发布")
            .style(ButtonStyle::Success);

        let cancel_btn = CreateButton::new("cancel_auto_publish")
            .label("❌ 取消")
            .style(ButtonStyle::Danger);

        let action_row = CreateActionRow::Buttons(vec![confirm_btn, cancel_btn]);

        let message = CreateMessage::new()
            .embed(embed)
            .components(vec![action_row]);

        let sent_message = ChannelId::new(self.thread.id.get())
            .send_message(&self.ctx.http, message)
            .await?;

        self.current_message = Some(sent_message);
        Ok(())
    }

    /// 处理等待新用户选择状态
    async fn handle_awaiting_guidance_state(&mut self) -> Result<bool, BotError> {
        // 构建引导消息和按钮
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

        let sent_message = ChannelId::new(self.thread.id.get())
            .send_message(&self.ctx.http, message)
            .await?;

        self.current_message = Some(sent_message.clone());

        // 等待用户交互
        let Some(interaction) = sent_message
            .await_component_interaction(&self.ctx.shard)
            .author_id(self.owner_id)
            .timeout(std::time::Duration::from_secs(180))
            .await
        else {
            // 超时，转到完成状态
            self.transition_to(FlowState::Done);
            return Ok(true);
        };

        match interaction.data.custom_id.as_str() {
            "enable_auto_publish_setup" => {
                // 用户选择启用功能
                self.data.db().user_settings().set_auto_publish(self.owner_id, true).await?;
                self.transition_to(FlowState::SelectingLicense);
                Ok(true)
            }
            "disable_auto_publish_setup" => {
                // 用户选择关闭功能
                self.data.db().user_settings().set_auto_publish(self.owner_id, false).await?;
                
                // 礼貌回复
                interaction
                    .create_response(
                        &self.ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("好的，如果你改变主意，可以随时使用 `/自动发布设置` 手动开启。")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
                
                self.transition_to(FlowState::Done);
                Ok(true)
            }
            _ => {
                self.transition_to(FlowState::Done);
                Ok(true)
            }
        }
    }

    /// 处理选择协议状态
    async fn handle_selecting_license_state(&mut self) -> Result<bool, BotError> {
        // 获取系统协议并缓存
        let system_licenses = self.data.system_license_cache().get_all().await;
        self.system_licenses = Some(system_licenses.clone());

        // 创建选择菜单
        let mut select_options = vec![
            CreateSelectMenuOption::new("创建新协议", "new_license")
                .description("创建一个全新的协议")
        ];
        
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

        // 发送选择菜单
        if let Some(ref mut message) = self.current_message {
            // 编辑现有消息
            message.edit(
                &self.ctx.http,
                serenity::all::EditMessage::new()
                    .content("请选择你要使用的协议：")
                    .components(vec![CreateActionRow::SelectMenu(select_menu)])
            ).await?;
        } else {
            // 创建新消息
            let new_message = CreateMessage::new()
                .content("请选择你要使用的协议：")
                .components(vec![CreateActionRow::SelectMenu(select_menu)]);

            let sent_message = ChannelId::new(self.thread.id.get())
                .send_message(&self.ctx.http, new_message)
                .await?;

            self.current_message = Some(sent_message);
        }

        // 等待用户选择
        let Some(select_interaction) = self.current_message
            .as_ref()
            .unwrap()
            .await_component_interaction(&self.ctx.shard)
            .author_id(self.owner_id)
            .timeout(std::time::Duration::from_secs(120))
            .await
        else {
            self.transition_to(FlowState::Done);
            return Ok(true);
        };

        // 处理用户选择
        if let ComponentInteractionDataKind::StringSelect { values } = &select_interaction.data.kind {
            if let Some(selected) = values.first() {
                let initial_state = if selected == "new_license" {
                    LicenseEditState::new("新协议".to_string())
                } else if let Some(system_name) = selected.strip_prefix("system_") {
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

                self.transition_to(FlowState::EditingLicense(initial_state));
                Ok(true)
            } else {
                self.transition_to(FlowState::Done);
                Ok(true)
            }
        } else {
            self.transition_to(FlowState::Done);
            Ok(true)
        }
    }

    /// 处理编辑协议状态
    async fn handle_editing_license_state(&mut self, edit_state: LicenseEditState) -> Result<bool, BotError> {
        // 需要一个临时的ComponentInteraction来启动编辑面板
        // 这里需要从当前消息获取最后一个交互
        let message = self.current_message.as_ref().unwrap();
        
        // 等待下一个交互以启动编辑面板
        let Some(interaction) = message
            .await_component_interaction(&self.ctx.shard)
            .author_id(self.owner_id)
            .timeout(std::time::Duration::from_secs(300))
            .await
        else {
            self.transition_to(FlowState::Done);
            return Ok(true);
        };

        // 调用协议编辑面板
        match present_license_editing_panel(self.ctx, self.data, &interaction, edit_state).await {
            Ok(Some(final_state)) => {
                // 用户保存了协议
                match self.save_license_and_set_default(final_state).await {
                    Ok(license) => {
                        self.transition_to(FlowState::ConfirmingSave(license));
                        Ok(true)
                    }
                    Err(e) => {
                        tracing::error!("保存协议失败: {}", e);
                        // 发送错误消息
                        interaction
                            .create_followup(
                                &self.ctx.http,
                                CreateInteractionResponseFollowup::new()
                                    .content("❌ 协议保存失败，请稍后重试。")
                                    .ephemeral(true),
                            )
                            .await?;
                        self.transition_to(FlowState::Done);
                        Ok(true)
                    }
                }
            }
            Ok(None) => {
                // 用户取消了编辑
                interaction
                    .create_followup(
                        &self.ctx.http,
                        CreateInteractionResponseFollowup::new()
                            .content("已取消协议创建。自动发布功能已启用，但您需要手动设置默认协议。")
                            .ephemeral(true),
                    )
                    .await?;
                self.transition_to(FlowState::Done);
                Ok(true)
            }
            Err(e) => {
                tracing::error!("协议编辑流程失败: {}", e);
                self.transition_to(FlowState::Done);
                Err(e)
            }
        }
    }

    /// 处理确认保存协议状态
    async fn handle_confirming_save_state(&mut self, license: crate::services::license::UserLicense) -> Result<bool, BotError> {
        // 直接转到确认发布状态
        self.transition_to(FlowState::ConfirmingPublish(license));
        Ok(true)
    }

    /// 处理确认发布协议状态
    async fn handle_confirming_publish_state(&mut self, license: crate::services::license::UserLicense) -> Result<bool, BotError> {
        // 如果是来自初始状态的确认发布，需要等待用户交互
        if let Some(message) = &self.current_message {
            let Some(interaction) = message
                .await_component_interaction(&self.ctx.shard)
                .author_id(self.owner_id)
                .timeout(std::time::Duration::from_secs(180))
                .await
            else {
                // 超时，删除消息
                self.transition_to(FlowState::Done);
                return Ok(true);
            };

            match interaction.data.custom_id.as_str() {
                "confirm_auto_publish" => {
                    // 确认发布
                    self.publish_license_directly(&license).await?;
                    
                    // 删除交互面板
                    let _ = message.delete(&self.ctx.http).await;
                    self.current_message = None;

                    // 回应交互
                    interaction
                        .create_response(
                            &self.ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content("✅ 协议已成功发布！")
                                    .ephemeral(true),
                            ),
                        )
                        .await?;
                }
                "cancel_auto_publish" => {
                    // 取消发布
                    let _ = message.delete(&self.ctx.http).await;
                    self.current_message = None;

                    // 回应交互
                    interaction
                        .create_response(
                            &self.ctx.http,
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
        } else {
            // 来自新用户流程的发布确认
            self.show_new_user_publish_confirmation(&license).await?;
        }

        self.transition_to(FlowState::Done);
        Ok(true)
    }

    /// 显示新用户发布确认
    async fn show_new_user_publish_confirmation(&mut self, license: &crate::services::license::UserLicense) -> Result<(), BotError> {
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
        
        let message = CreateMessage::new()
            .content(confirm_message)
            .components(vec![action_row]);

        let mut sent_message = ChannelId::new(self.thread.id.get())
            .send_message(&self.ctx.http, message)
            .await?;

        self.current_message = Some(sent_message.clone());

        // 等待用户交互
        let Some(publish_interaction) = sent_message
            .await_component_interaction(&self.ctx.shard)
            .author_id(self.owner_id)
            .timeout(std::time::Duration::from_secs(120))
            .await
        else {
            // 超时，编辑为最终状态
            sent_message.edit(
                &self.ctx.http,
                serenity::all::EditMessage::new()
                    .content("协议已创建并设置为默认协议！自动发布功能现在已完全启用。")
                    .components(Vec::new())
            ).await?;
            return Ok(());
        };

        match publish_interaction.data.custom_id.as_str() {
            "confirm_publish_new_license" => {
                // 发布协议
                self.publish_license_directly(license).await?;
                
                // 确认发布成功
                publish_interaction
                    .create_response(
                        &self.ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("✅ 协议已成功发布到当前帖子！")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
                    
                // 编辑为最终状态
                sent_message.edit(
                    &self.ctx.http,
                    serenity::all::EditMessage::new()
                        .content("协议已创建、设置为默认协议，并发布到当前帖子！")
                        .components(Vec::new())
                ).await?;
            }
            "skip_publish_new_license" => {
                // 不发布
                publish_interaction
                    .create_response(
                        &self.ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("好的，协议已保存。你可以稍后手动发布或在新帖子中自动发布。")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
                    
                // 编辑为最终状态
                sent_message.edit(
                    &self.ctx.http,
                    serenity::all::EditMessage::new()
                        .content("协议已创建并设置为默认协议！自动发布功能现在已完全启用。")
                        .components(Vec::new())
                ).await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// 保存协议并设置为默认协议
    async fn save_license_and_set_default(
        &self,
        final_state: LicenseEditState,
    ) -> Result<crate::services::license::UserLicense, BotError> {
        let (name, allow_redistribution, allow_modification, restrictions_note, allow_backup) = 
            final_state.to_user_license_fields();
        
        // 创建协议
        let license = self.data.db().license().create(
            self.owner_id,
            name,
            allow_redistribution,
            allow_modification,
            restrictions_note,
            allow_backup,
        ).await?;
        
        // 设置为默认协议
        self.data.db().user_settings().set_default_license(
            self.owner_id,
            Some(DefaultLicenseIdentifier::User(license.id)),
            None,
        ).await?;
        
        Ok(license)
    }
}