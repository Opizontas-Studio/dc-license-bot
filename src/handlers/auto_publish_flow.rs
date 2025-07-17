use serenity::all::{
    ButtonStyle, ChannelId, ComponentInteractionDataKind, Context, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseFollowup, CreateInteractionResponseMessage,
    CreateMessage, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, GuildChannel,
    Message, UserId,
};

use crate::{
    commands::Data,
    error::BotError,
    services::license::LicensePublishService,
    types::license::DefaultLicenseIdentifier,
    utils::{LicenseEditState, LicenseEmbedBuilder, present_license_editing_panel},
};

/// 自动发布流程的状态定义
#[derive(Debug, Clone)]
pub enum FlowState {
    /// 初始状态 - 检查用户设置并决定后续流程
    Initial,
    /// 等待新用户选择启用/禁用功能
    AwaitingGuidance,
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
    /// 当前等待的交互
    pending_interaction: Option<serenity::all::ComponentInteraction>,
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
            pending_interaction: None,
        }
    }

    /// 运行状态机主循环
    pub async fn run(mut self) -> Result<(), BotError> {
        loop {
            tracing::debug!("处理状态: {:?}", self.state);

            match self.state {
                FlowState::Initial => {
                    // 初始状态 - 检查用户设置并决定后续流程
                    if let Err(e) = self.handle_initial_state().await {
                        tracing::error!("初始状态处理错误: {}", e);
                        self.cleanup().await;
                        return Err(e);
                    }
                }
                FlowState::AwaitingGuidance => {
                    // 等待新用户选择启用/禁用
                    match self.handle_awaiting_guidance().await {
                        Ok(()) => {} // 继续到下一个状态
                        Err(e) => {
                            tracing::error!("等待引导状态处理错误: {}", e);
                            self.cleanup().await;
                            return Err(e);
                        }
                    }
                }
                FlowState::EditingLicense(ref edit_state) => {
                    // 编辑协议
                    let edit_state = edit_state.clone();
                    match self.handle_editing_license(edit_state).await {
                        Ok(()) => {} // 继续到下一个状态
                        Err(e) => {
                            tracing::error!("编辑协议状态处理错误: {}", e);
                            self.cleanup().await;
                            return Err(e);
                        }
                    }
                }
                FlowState::ConfirmingSave(ref license) => {
                    // 确认保存协议
                    let license = license.clone();
                    match self.handle_confirming_save(license).await {
                        Ok(()) => {} // 继续到下一个状态
                        Err(e) => {
                            tracing::error!("确认保存状态处理错误: {}", e);
                            self.cleanup().await;
                            return Err(e);
                        }
                    }
                }
                FlowState::ConfirmingPublish(ref license) => {
                    // 确认发布协议
                    let license = license.clone();
                    match self.handle_confirming_publish(license).await {
                        Ok(()) => {} // 继续到下一个状态
                        Err(e) => {
                            tracing::error!("确认发布状态处理错误: {}", e);
                            self.cleanup().await;
                            return Err(e);
                        }
                    }
                }
                FlowState::Done => {
                    // 完成状态，退出循环
                    break;
                }
            }
        }

        // 正常完成，清理资源
        self.cleanup().await;
        Ok(())
    }

    /// 等待用户交互，统一的交互处理方法
    async fn wait_for_interaction(
        &mut self,
        timeout_secs: u64,
    ) -> Result<Option<serenity::all::ComponentInteraction>, BotError> {
        if let Some(message) = &self.current_message {
            let interaction = message
                .await_component_interaction(&self.ctx.shard)
                .author_id(self.owner_id)
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .await;

            if let Some(interaction) = interaction {
                self.pending_interaction = Some(interaction.clone());
                Ok(Some(interaction))
            } else {
                // 超时，转到完成状态
                tracing::debug!("用户交互超时，转换到完成状态");
                self.transition_to(FlowState::Done);
                Ok(None)
            }
        } else {
            Err(BotError::GenericError {
                message: "没有当前消息可等待交互".to_string(),
                source: None,
            })
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
    async fn handle_initial_state(&mut self) -> Result<(), BotError> {
        // 检查用户设置状态
        let user_settings = self.data.db().user_settings().get(self.owner_id).await?;

        match user_settings {
            // 场景一：新用户
            None => {
                self.transition_to(FlowState::AwaitingGuidance);
            }
            // 用户已存在
            Some(settings) => {
                if !settings.auto_publish_enabled {
                    // 场景三：已关闭功能的用户，静默退出
                    self.transition_to(FlowState::Done);
                    return Ok(());
                }

                // 场景二：已启用功能的用户
                let default_license_id = if let Some(user_license_id) =
                    settings.default_user_license_id
                {
                    DefaultLicenseIdentifier::User(user_license_id)
                } else if let Some(ref system_license_name) = settings.default_system_license_name {
                    DefaultLicenseIdentifier::System(system_license_name.clone())
                } else {
                    // 用户启用了功能但未设置默认协议，静默退出
                    self.transition_to(FlowState::Done);
                    return Ok(());
                };

                // 根据协议ID获取完整的协议内容
                let license_model = self
                    .get_license_model(&default_license_id, &settings)
                    .await?;

                if let Some(license) = license_model {
                    // 检查是否跳过确认
                    if settings.skip_auto_publish_confirmation {
                        // 直接发布协议
                        self.publish_license_directly(&license).await?;
                        self.transition_to(FlowState::Done);
                    } else {
                        // 显示确认面板
                        self.show_auto_publish_confirmation(&license).await?;
                        self.transition_to(FlowState::ConfirmingPublish(license));
                    }
                } else {
                    // 协议不存在，静默退出
                    self.transition_to(FlowState::Done);
                }
            }
        }

        Ok(())
    }

    /// 获取协议模型
    async fn get_license_model(
        &self,
        license_id: &DefaultLicenseIdentifier,
        settings: &entities::entities::user_settings::Model,
    ) -> Result<Option<crate::services::license::UserLicense>, BotError> {
        match license_id {
            DefaultLicenseIdentifier::User(id) => Ok(self
                .data
                .db()
                .license()
                .get_license(*id, self.owner_id)
                .await?),
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
    async fn publish_license_directly(
        &self,
        license: &crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
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
    async fn show_auto_publish_confirmation(
        &mut self,
        license: &crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
        let display_name = self
            .thread
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
    async fn handle_awaiting_guidance(&mut self) -> Result<(), BotError> {
        // 构建引导消息和按钮
        let welcome_message =
            "你好！我们发现你发了一个新帖子。你是否想开启'自动添加许可协议'的功能呢？";

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

        self.current_message = Some(sent_message);

        // 等待用户交互
        let Some(interaction) = self.wait_for_interaction(180).await? else {
            // 超时已在wait_for_interaction中处理
            return Ok(());
        };

        match interaction.data.custom_id.as_str() {
            "enable_auto_publish_setup" => {
                // 用户选择启用功能
                self.data
                    .db()
                    .user_settings()
                    .set_auto_publish(self.owner_id, true)
                    .await?;

                // 获取协议数据
                let system_licenses = self.data.system_license_cache().get_all().await;
                self.system_licenses = Some(system_licenses.clone());

                // 创建选择菜单
                let mut select_options = vec![
                    CreateSelectMenuOption::new("创建新协议", "new_license")
                        .description("创建一个全新的协议"),
                ];

                for license in &system_licenses {
                    select_options.push(
                        CreateSelectMenuOption::new(
                            &license.license_name,
                            format!("system_{}", license.license_name),
                        )
                        .description("基于系统协议创建"),
                    );
                }

                let select_menu = CreateSelectMenu::new(
                    "license_selection",
                    CreateSelectMenuKind::String {
                        options: select_options,
                    },
                )
                .placeholder("请选择协议类型")
                .max_values(1);

                // 立即确认交互并附加选择菜单 - 全部 ephemeral
                interaction
                    .create_response(
                        &self.ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("✅ 自动发布功能已启用！\n\n请选择你要使用的协议：")
                                .components(vec![CreateActionRow::SelectMenu(select_menu)])
                                .ephemeral(true),
                        ),
                    )
                    .await?;

                // 删除旧的引导消息
                if let Some(message) = &self.current_message {
                    let _ = message.delete(&self.ctx.http).await;
                    self.current_message = None;
                }

                // 等待用户选择协议 - 直接在这里处理
                let Some(select_interaction) = interaction
                    .get_response(&self.ctx.http)
                    .await?
                    .await_component_interaction(&self.ctx.shard)
                    .author_id(self.owner_id)
                    .timeout(std::time::Duration::from_secs(120))
                    .await
                else {
                    // 超时，转到完成状态
                    self.transition_to(FlowState::Done);
                    return Ok(());
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

                        // 保存选择交互并转换状态
                        self.pending_interaction = Some(select_interaction);
                        self.transition_to(FlowState::EditingLicense(initial_state));
                    } else {
                        self.transition_to(FlowState::Done);
                    }
                } else {
                    self.transition_to(FlowState::Done);
                }
            }
            "disable_auto_publish_setup" => {
                // 用户选择关闭功能
                self.data
                    .db()
                    .user_settings()
                    .set_auto_publish(self.owner_id, false)
                    .await?;

                // 礼貌回复
                interaction
                    .create_response(
                        &self.ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(
                                    "好的，如果你改变主意，可以随时使用 `/自动发布设置` 手动开启。",
                                )
                                .ephemeral(true),
                        ),
                    )
                    .await?;

                self.transition_to(FlowState::Done);
            }
            _ => {
                self.transition_to(FlowState::Done);
            }
        }

        Ok(())
    }


    /// 处理编辑协议状态
    async fn handle_editing_license(
        &mut self,
        edit_state: LicenseEditState,
    ) -> Result<(), BotError> {
        // 使用pending_interaction（从上一个状态保存的交互）
        let interaction =
            self.pending_interaction
                .take()
                .ok_or_else(|| BotError::GenericError {
                    message: "没有可用的交互来启动编辑面板".to_string(),
                    source: None,
                })?;

        // 调用协议编辑面板
        match present_license_editing_panel(self.ctx, self.data, &interaction, edit_state).await {
            Ok(Some(final_state)) => {
                // 用户保存了协议
                match self.save_license_and_set_default(final_state).await {
                    Ok(license) => {
                        self.transition_to(FlowState::ConfirmingSave(license));
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
                    }
                }
            }
            Ok(None) => {
                // 用户取消了编辑
                interaction
                    .create_followup(
                        &self.ctx.http,
                        CreateInteractionResponseFollowup::new()
                            .content(
                                "已取消协议创建。自动发布功能已启用，但您需要手动设置默认协议。",
                            )
                            .ephemeral(true),
                    )
                    .await?;
                self.transition_to(FlowState::Done);
            }
            Err(e) => {
                tracing::error!("协议编辑流程失败: {}", e);
                self.transition_to(FlowState::Done);
                return Err(e);
            }
        }

        Ok(())
    }

    /// 处理确认保存协议状态
    async fn handle_confirming_save(
        &mut self,
        license: crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
        // 直接转到确认发布状态
        self.transition_to(FlowState::ConfirmingPublish(license));
        Ok(())
    }

    /// 处理确认发布协议状态
    async fn handle_confirming_publish(
        &mut self,
        license: crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
        // 如果是来自初始状态的确认发布，需要等待用户交互
        if let Some(_message) = &self.current_message {
            let Some(interaction) = self.wait_for_interaction(180).await? else {
                // 超时已在wait_for_interaction中处理
                return Ok(());
            };

            match interaction.data.custom_id.as_str() {
                "confirm_auto_publish" => {
                    // 确认发布
                    self.publish_license_directly(&license).await?;

                    // 删除交互面板
                    if let Some(message) = &self.current_message {
                        let _ = message.delete(&self.ctx.http).await;
                    }
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
                    if let Some(message) = &self.current_message {
                        let _ = message.delete(&self.ctx.http).await;
                    }
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

            // 等待用户交互
            let Some(interaction) = self.wait_for_interaction(120).await? else {
                // 超时已在wait_for_interaction中处理
                return Ok(());
            };

            match interaction.data.custom_id.as_str() {
                "confirm_publish_new_license" => {
                    // 发布协议
                    self.publish_license_directly(&license).await?;

                    // 确认发布成功
                    interaction
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
                    if let Some(message) = &self.current_message {
                        let message_id = message.id;
                        ChannelId::new(self.thread.id.get())
                            .edit_message(
                                &self.ctx.http,
                                message_id,
                                serenity::all::EditMessage::new()
                                    .content("协议已创建、设置为默认协议，并发布到当前帖子！")
                                    .components(Vec::new()),
                            )
                            .await?;
                    }
                }
                "skip_publish_new_license" => {
                    // 不发布
                    interaction
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
                    if let Some(message) = &self.current_message {
                        let message_id = message.id;
                        ChannelId::new(self.thread.id.get())
                            .edit_message(
                                &self.ctx.http,
                                message_id,
                                serenity::all::EditMessage::new()
                                    .content(
                                        "协议已创建并设置为默认协议！自动发布功能现在已完全启用。",
                                    )
                                    .components(Vec::new()),
                            )
                            .await?;
                    }
                }
                _ => {}
            }
        }

        self.transition_to(FlowState::Done);
        Ok(())
    }

    /// 显示新用户发布确认
    async fn show_new_user_publish_confirmation(
        &mut self,
        license: &crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
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

        let sent_message = ChannelId::new(self.thread.id.get())
            .send_message(&self.ctx.http, message)
            .await?;

        self.current_message = Some(sent_message);
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
        let license = self
            .data
            .db()
            .license()
            .create(
                self.owner_id,
                name,
                allow_redistribution,
                allow_modification,
                restrictions_note,
                allow_backup,
            )
            .await?;

        // 设置为默认协议
        self.data
            .db()
            .user_settings()
            .set_default_license(
                self.owner_id,
                Some(DefaultLicenseIdentifier::User(license.id)),
                None,
            )
            .await?;

        Ok(license)
    }
}
