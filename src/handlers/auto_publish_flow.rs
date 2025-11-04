use chrono::Utc;
use serenity::all::{
    ChannelId, ComponentInteractionDataKind, Context, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, GuildChannel, Message,
    UserId,
};

use crate::{
    commands::Data,
    error::BotError,
    services::license::LicensePublishService,
    types::license::DefaultLicenseIdentifier,
    utils::{AutoPublishUI, LicenseEditState, present_license_editing_panel},
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
    /// 等待重新选择协议状态，包含系统协议缓存
    AwaitingLicenseReselection(Vec<crate::types::license::SystemLicense>),
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
    /// 编辑器交互（用于新用户流程的followup）
    editor_interaction: Option<serenity::all::ComponentInteraction>,
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
            editor_interaction: None,
        }
    }

    /// 运行状态机主循环
    pub async fn run(mut self) -> Result<(), BotError> {
        loop {
            tracing::debug!("处理状态: {:?}", self.state);

            let result = match self.state {
                FlowState::Initial => self.handle_initial_state().await,
                FlowState::AwaitingGuidance => self.handle_awaiting_guidance().await,
                FlowState::EditingLicense(ref edit_state) => {
                    let edit_state = edit_state.clone();
                    self.handle_editing_license(edit_state).await
                }
                FlowState::AwaitingLicenseReselection(ref system_licenses) => {
                    let system_licenses = system_licenses.clone();
                    self.handle_awaiting_license_reselection(system_licenses)
                        .await
                }
                FlowState::ConfirmingSave(ref license) => {
                    let license = license.clone();
                    self.handle_confirming_save(license).await
                }
                FlowState::ConfirmingPublish(ref license) => {
                    let license = license.clone();
                    self.handle_confirming_publish(license).await
                }
                FlowState::Done => {
                    break;
                }
            };

            if let Err(e) = result {
                self.handle_state_error(&e).await;
                return Err(e);
            }
        }

        // 正常完成，清理资源
        self.cleanup().await;
        Ok(())
    }

    /// 统一的状态错误处理
    async fn handle_state_error(&mut self, error: &BotError) {
        tracing::error!("状态机处理错误: {}", error);
        self.cleanup().await;
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

    /// 等待交互或超时结束，统一处理超时逻辑
    async fn wait_for_interaction_or_finish(
        &mut self,
        timeout_secs: u64,
    ) -> Result<Option<serenity::all::ComponentInteraction>, BotError> {
        match self.wait_for_interaction(timeout_secs).await? {
            Some(interaction) => Ok(Some(interaction)),
            None => {
                // 已经在wait_for_interaction中转换到Done状态
                Ok(None)
            }
        }
    }

    /// 等待followup交互或超时结束，统一处理超时逻辑
    async fn wait_for_followup_interaction_or_finish(
        &self,
        followup_message: &Message,
        timeout_secs: u64,
    ) -> Result<Option<serenity::all::ComponentInteraction>, BotError> {
        match self
            .wait_for_followup_interaction(followup_message, timeout_secs)
            .await?
        {
            Some(interaction) => Ok(Some(interaction)),
            None => {
                // 超时，记录日志但不在这里转换状态（由调用者处理）
                tracing::debug!("Followup交互超时");
                Ok(None)
            }
        }
    }

    /// 转换到新状态
    fn transition_to(&mut self, new_state: FlowState) {
        tracing::debug!("状态转换: {:?} -> {:?}", self.state, new_state);
        self.state = new_state;
    }

    /// 清理资源
    async fn cleanup(&mut self) {
        // 只清理需要删除的消息（通常是错误状态时的消息）
        // followup消息和已完成的消息不需要删除
        if let Some(message) = &self.current_message {
            // 只删除确认类型的消息，其他消息保留作为状态记录
            let _ = message.delete(&self.ctx.http).await;
        }
    }

    /// 统一的成功响应方法
    async fn respond_with_success(
        &self,
        interaction: &serenity::all::ComponentInteraction,
        message: &str,
    ) -> Result<(), BotError> {
        interaction
            .create_response(
                &self.ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(message)
                        .ephemeral(true),
                ),
            )
            .await?;
        Ok(())
    }

    /// 统一的错误followup方法
    async fn followup_with_error(
        &self,
        interaction: &serenity::all::ComponentInteraction,
        message: &str,
    ) -> Result<(), BotError> {
        interaction
            .create_followup(
                &self.ctx.http,
                CreateInteractionResponseFollowup::new()
                    .content(format!("❌ {message}"))
                    .ephemeral(true),
            )
            .await?;
        Ok(())
    }

    /// 清理消息并响应
    async fn cleanup_message_and_respond(
        &mut self,
        interaction: &serenity::all::ComponentInteraction,
        response: CreateInteractionResponseMessage,
    ) -> Result<(), BotError> {
        // 删除当前消息
        if let Some(message) = &self.current_message {
            let _ = message.delete(&self.ctx.http).await;
        }
        self.current_message = None;

        // 响应交互
        interaction
            .create_response(&self.ctx.http, CreateInteractionResponse::Message(response))
            .await?;
        Ok(())
    }

    /// 从followup消息等待交互
    async fn wait_for_followup_interaction(
        &self,
        followup_message: &Message,
        timeout_secs: u64,
    ) -> Result<Option<serenity::all::ComponentInteraction>, BotError> {
        let interaction = followup_message
            .await_component_interaction(&self.ctx.shard)
            .author_id(self.owner_id)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .await;

        Ok(interaction)
    }

    /// 处理初始状态 - 检查用户设置并决定后续流程
    async fn handle_initial_state(&mut self) -> Result<(), BotError> {
        // 检查帖子创建时间，防止处理bot部署前的旧帖子
        if let Some(thread_metadata) = &self.thread.thread_metadata
            && let Some(create_timestamp) = thread_metadata.create_timestamp
        {
            let bot_start_time = self.data.cfg().load().bot_start_time;

            // 如果帖子创建时间早于bot启动时间，静默退出
            if create_timestamp.timestamp() < bot_start_time.timestamp() {
                tracing::debug!(
                    "跳过旧帖子处理: 帖子创建于 {}, bot启动于 {}",
                    create_timestamp,
                    bot_start_time
                );
                self.transition_to(FlowState::Done);
                return Ok(());
            }

            // 额外检查：检查首楼消息时间，确保是真正的新帖子
            let now = Utc::now();
            let thread_age_secs = now.timestamp() - create_timestamp.timestamp();
            if thread_age_secs > 300 {
                tracing::debug!(
                    "跳过过期帖子处理: 帖子创建于 {} ({} 秒前)",
                    create_timestamp,
                    thread_age_secs
                );
                self.transition_to(FlowState::Done);
                return Ok(());
            }
        }

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

        // 使用UI构建器创建确认面板
        let message = AutoPublishUI::build_auto_publish_confirmation(license, &display_name);

        let sent_message = ChannelId::new(self.thread.id.get())
            .send_message(&self.ctx.http, message)
            .await?;

        self.current_message = Some(sent_message);
        Ok(())
    }

    /// 处理等待新用户选择状态
    async fn handle_awaiting_guidance(&mut self) -> Result<(), BotError> {
        // 使用UI构建器创建引导消息
        let message = AutoPublishUI::build_guidance_message();

        let sent_message = ChannelId::new(self.thread.id.get())
            .send_message(&self.ctx.http, message)
            .await?;

        self.current_message = Some(sent_message);

        // 等待用户交互
        let Some(interaction) = self.wait_for_interaction_or_finish(180).await? else {
            return Ok(());
        };

        match interaction.data.custom_id.as_str() {
            "enable_auto_publish_setup" => {
                self.handle_enable_setup(interaction).await?;
            }
            "disable_auto_publish_setup" => {
                self.handle_disable_setup(interaction).await?;
            }
            _ => {
                self.transition_to(FlowState::Done);
            }
        }

        Ok(())
    }

    /// 处理启用自动发布设置
    async fn handle_enable_setup(
        &mut self,
        interaction: serenity::all::ComponentInteraction,
    ) -> Result<(), BotError> {
        // 获取协议数据
        let system_licenses = self.data.system_license_cache().get_all().await;
        self.system_licenses = Some(system_licenses.clone());

        // 使用UI构建器创建选择菜单
        let select_menu = AutoPublishUI::build_license_selection_menu(&system_licenses);

        // 立即确认交互并附加选择菜单 - 全部 ephemeral
        interaction
            .create_response(
                &self.ctx.http,
                CreateInteractionResponse::Message(AutoPublishUI::create_enable_response(
                    select_menu,
                )),
            )
            .await?;

        // 删除旧的引导消息
        if let Some(message) = &self.current_message {
            let _ = message.delete(&self.ctx.http).await;
            self.current_message = None;
        }

        // 等待用户选择协议
        self.handle_license_selection(interaction, system_licenses)
            .await?;

        Ok(())
    }

    /// 处理协议选择
    async fn handle_license_selection(
        &mut self,
        interaction: serenity::all::ComponentInteraction,
        system_licenses: Vec<crate::types::license::SystemLicense>,
    ) -> Result<(), BotError> {
        // 等待用户选择协议
        let followup_message = interaction.get_response(&self.ctx.http).await?;
        let Some(select_interaction) = self
            .wait_for_followup_interaction_or_finish(&followup_message, 120)
            .await?
        else {
            self.transition_to(FlowState::Done);
            return Ok(());
        };

        // 处理用户选择
        if let ComponentInteractionDataKind::StringSelect { values } = &select_interaction.data.kind
        {
            if let Some(selected) = values.first() {
                let initial_state = self
                    .create_license_edit_state(selected, &system_licenses)
                    .await?;

                // 保存选择交互并转换状态
                self.pending_interaction = Some(select_interaction);
                self.transition_to(FlowState::EditingLicense(initial_state));
            } else {
                self.transition_to(FlowState::Done);
            }
        } else {
            self.transition_to(FlowState::Done);
        }

        Ok(())
    }

    /// 根据选择创建编辑状态
    async fn create_license_edit_state(
        &self,
        selected: &str,
        system_licenses: &[crate::types::license::SystemLicense],
    ) -> Result<LicenseEditState, BotError> {
        if selected == "new_license" {
            // 使用智能命名策略，避免重名协议
            let user_licenses = self
                .data
                .db()
                .license()
                .get_user_licenses(self.owner_id)
                .await?;
            let next_number = user_licenses.len() + 1;
            let default_name = format!("我的协议{next_number}");
            Ok(LicenseEditState::new(default_name))
        } else if let Some(system_name) = selected.strip_prefix("system_") {
            if let Some(system_license) = system_licenses
                .iter()
                .find(|l| l.license_name == system_name)
            {
                Ok(LicenseEditState::from_system_license(system_license))
            } else {
                Err(BotError::GenericError {
                    message: "选择的系统协议不存在".to_string(),
                    source: None,
                })
            }
        } else {
            Err(BotError::GenericError {
                message: "无效的选择".to_string(),
                source: None,
            })
        }
    }

    /// 处理禁用自动发布设置
    async fn handle_disable_setup(
        &mut self,
        interaction: serenity::all::ComponentInteraction,
    ) -> Result<(), BotError> {
        // 禁用自动发布功能
        self.data
            .db()
            .user_settings()
            .set_auto_publish(self.owner_id, false)
            .await?;

        // 礼貌回复
        interaction
            .create_response(
                &self.ctx.http,
                CreateInteractionResponse::Message(AutoPublishUI::create_disable_response()),
            )
            .await?;

        self.transition_to(FlowState::Done);
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
            Ok(outcome) => {
                if let Some(final_state) = outcome.state {
                    let Some(latest_interaction) = outcome.interaction else {
                        tracing::warn!("协议编辑完成但缺少有效的交互令牌，终止后续流程");
                        self.transition_to(FlowState::Done);
                        return Ok(());
                    };

                    // 保存最新交互用于后续followup
                    self.editor_interaction = Some(latest_interaction.clone());

                    match self.save_license_and_set_default(final_state).await {
                        Ok(license) => {
                            self.transition_to(FlowState::ConfirmingSave(license));
                        }
                        Err(e) => {
                            tracing::error!("保存协议失败: {}", e);
                            // 发送错误消息
                            self.followup_with_error(
                                &latest_interaction,
                                "协议保存失败，请稍后重试。",
                            )
                            .await?;
                            self.transition_to(FlowState::Done);
                        }
                    }
                } else if let Some(latest_interaction) = outcome.interaction {
                    // 用户取消了编辑，转到重新选择状态
                    let system_licenses = self.system_licenses.clone().unwrap_or_default();
                    self.editor_interaction = Some(latest_interaction);
                    self.transition_to(FlowState::AwaitingLicenseReselection(system_licenses));
                } else {
                    // 没有新的交互（例如超时），结束流程
                    self.transition_to(FlowState::Done);
                }
            }
            Err(e) => {
                tracing::error!("协议编辑流程失败: {}", e);
                self.transition_to(FlowState::Done);
                return Err(e);
            }
        }

        Ok(())
    }

    /// 处理等待重新选择协议状态
    async fn handle_awaiting_license_reselection(
        &mut self,
        system_licenses: Vec<crate::types::license::SystemLicense>,
    ) -> Result<(), BotError> {
        // 使用保存的编辑器交互来发送重新选择菜单
        let editor_interaction =
            self.editor_interaction
                .take()
                .ok_or_else(|| BotError::GenericError {
                    message: "没有可用的编辑器交互来显示重新选择菜单".to_string(),
                    source: None,
                })?;

        // 显示重新选择菜单
        let followup_message = editor_interaction
            .create_followup(
                &self.ctx.http,
                AutoPublishUI::build_license_reselection_menu(&system_licenses),
            )
            .await?;

        // 等待用户重新选择
        let Some(reselect_interaction) = self
            .wait_for_followup_interaction_or_finish(&followup_message, 120)
            .await?
        else {
            self.transition_to(FlowState::Done);
            return Ok(());
        };

        // 处理用户重新选择
        if let ComponentInteractionDataKind::StringSelect { values } =
            &reselect_interaction.data.kind
        {
            if let Some(selected) = values.first() {
                match selected.as_str() {
                    "exit_setup" => {
                        // 用户选择退出
                        self.respond_with_success(
                            &reselect_interaction,
                            "好的，如果你改变主意，可以随时使用 `/自动发布设置` 手动开启。",
                        )
                        .await?;
                        self.transition_to(FlowState::Done);
                    }
                    _ => {
                        // 用户选择了协议，重新进入编辑状态
                        let initial_state = self
                            .create_license_edit_state(selected, &system_licenses)
                            .await?;
                        self.pending_interaction = Some(reselect_interaction);
                        self.transition_to(FlowState::EditingLicense(initial_state));
                    }
                }
            } else {
                self.transition_to(FlowState::Done);
            }
        } else {
            self.transition_to(FlowState::Done);
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
        // 判断是来自初始状态还是新用户流程
        if self.current_message.is_some() {
            // 来自初始状态的确认发布
            self.handle_existing_user_publish_confirmation(license)
                .await?;
        } else {
            // 来自新用户流程的发布确认
            self.handle_new_user_publish_confirmation(license).await?;
        }

        self.transition_to(FlowState::Done);
        Ok(())
    }

    /// 处理现有用户的发布确认
    async fn handle_existing_user_publish_confirmation(
        &mut self,
        license: crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
        let Some(interaction) = self.wait_for_interaction_or_finish(180).await? else {
            return Ok(());
        };

        match interaction.data.custom_id.as_str() {
            "confirm_auto_publish" => {
                // 确认发布
                self.publish_license_directly(&license).await?;
                self.cleanup_message_and_respond(
                    &interaction,
                    CreateInteractionResponseMessage::new()
                        .content("✅ 协议已成功发布！")
                        .ephemeral(true),
                )
                .await?;
            }
            "cancel_auto_publish" => {
                // 取消发布
                self.cleanup_message_and_respond(
                    &interaction,
                    AutoPublishUI::create_publish_cancel_response(),
                )
                .await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// 处理新用户的发布确认
    async fn handle_new_user_publish_confirmation(
        &mut self,
        license: crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
        let editor_interaction =
            self.editor_interaction
                .take()
                .ok_or_else(|| BotError::GenericError {
                    message: "没有可用的编辑器交互来显示确认".to_string(),
                    source: None,
                })?;

        let followup_message = self
            .show_new_user_publish_confirmation(&license, &editor_interaction)
            .await?;

        // 等待用户交互 - 从followup消息等待
        let Some(interaction) = self
            .wait_for_followup_interaction_or_finish(&followup_message, 120)
            .await?
        else {
            return Ok(());
        };

        match interaction.data.custom_id.as_str() {
            "confirm_publish_new_license" => {
                self.publish_and_respond_success(&interaction, &license)
                    .await?;
            }
            "skip_publish_new_license" => {
                self.respond_skip_publish(&interaction).await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// 发布协议并响应成功
    async fn publish_and_respond_success(
        &self,
        interaction: &serenity::all::ComponentInteraction,
        license: &crate::services::license::UserLicense,
    ) -> Result<(), BotError> {
        // 发布协议
        self.publish_license_directly(license).await?;

        // 直接编辑确认消息为最终状态，并响应interaction
        interaction
            .create_response(
                &self.ctx.http,
                CreateInteractionResponse::UpdateMessage(
                    serenity::all::CreateInteractionResponseMessage::new()
                        .content("✅ 协议已创建、设置为默认协议，并发布到当前帖子！")
                        .components(Vec::new()),
                ),
            )
            .await?;

        Ok(())
    }

    /// 响应跳过发布
    async fn respond_skip_publish(
        &self,
        interaction: &serenity::all::ComponentInteraction,
    ) -> Result<(), BotError> {
        // 直接编辑确认消息为最终状态，并响应interaction
        interaction
            .create_response(
                &self.ctx.http,
                CreateInteractionResponse::UpdateMessage(
                    serenity::all::CreateInteractionResponseMessage::new()
                        .content("✅ 协议已创建并设置为默认协议！你可以稍后使用 `/发布协议` 或在新帖子中自动发布。")
                        .components(Vec::new()),
                ),
            )
            .await?;

        Ok(())
    }

    /// 显示新用户发布确认（使用followup消息）
    async fn show_new_user_publish_confirmation(
        &mut self,
        license: &crate::services::license::UserLicense,
        interaction: &serenity::all::ComponentInteraction,
    ) -> Result<serenity::all::Message, BotError> {
        let followup_message = interaction
            .create_followup(
                &self.ctx.http,
                AutoPublishUI::create_new_license_publish_confirmation(&license.license_name),
            )
            .await?;

        Ok(followup_message)
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

        self.data
            .db()
            .user_settings()
            .set_auto_publish(self.owner_id, true)
            .await?;

        Ok(license)
    }
}
