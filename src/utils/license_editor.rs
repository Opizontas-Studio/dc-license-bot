use serenity::all::*;
use tracing::{debug, warn};

use super::editor_core::{EditorCore, LicenseEditState, UIProvider};
use crate::{commands::Data, error::BotError};

const INTERACTION_TIMEOUT_SECS: u64 = 600;

/// 协议编辑面板
///
/// 这个函数提供完整的协议编辑功能
///
/// # 参数
/// - `serenity_ctx`: Serenity 上下文
/// - `data`: 应用数据
/// - `interaction`: 组件交互
/// - `initial_state`: 初始编辑状态
///
/// # 返回值
/// 结果类型，包含最终状态与可复用的交互
#[derive(Debug, Clone)]
pub struct LicenseEditorOutcome {
    /// 用户最终保存的状态；为 `None` 表示取消或超时
    pub state: Option<LicenseEditState>,
    /// 最近一次组件交互，用于后续 follow-up 消息
    pub interaction: Option<ComponentInteraction>,
}

/// - `state: Some(LicenseEditState)`: 用户保存的最终状态
/// - `state: None`: 用户取消了编辑或超时
pub async fn present_license_editing_panel(
    serenity_ctx: &serenity::all::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    initial_state: LicenseEditState,
) -> Result<LicenseEditorOutcome, BotError> {
    // 创建编辑器状态
    let mut editor_state = LicenseEditor::new(serenity_ctx, data, initial_state);

    // 发送初始编辑界面
    editor_state.send_initial_ui(interaction).await?;

    // 主编辑循环 - 使用 tokio::select! 智能处理Modal和按钮交互
    loop {
        // 获取response对象用于监听交互
        let response = interaction.get_response(&serenity_ctx.http).await?;

        // 根据当前Modal状态决定监听策略
        match editor_state.modal_waiting {
            ModalWaitingState::None => {
                // 没有等待中的Modal，只等待按钮交互
                let Some(edit_interaction) = response
                    .await_component_interaction(&serenity_ctx.shard)
                    .author_id(interaction.user.id)
                    .timeout(std::time::Duration::from_secs(INTERACTION_TIMEOUT_SECS))
                    .await
                else {
                    // 超时，清理UI
                    editor_state.cleanup_ui(interaction).await?;
                    return Ok(LicenseEditorOutcome {
                        state: None,
                        interaction: None,
                    });
                };

                // 处理按钮交互
                let should_exit = editor_state.handle_interaction(&edit_interaction).await?;

                if should_exit {
                    // 检查是否是保存操作
                    if edit_interaction.data.custom_id == "save_license" {
                        editor_state.cleanup_ui(&edit_interaction).await?;
                        return Ok(LicenseEditorOutcome {
                            state: Some(editor_state.get_state().clone()),
                            interaction: Some(edit_interaction.clone()),
                        });
                    } else {
                        editor_state.cleanup_ui(&edit_interaction).await?;
                        return Ok(LicenseEditorOutcome {
                            state: None,
                            interaction: Some(edit_interaction.clone()),
                        });
                    }
                } else {
                    // 更新UI显示（如果不是Modal操作）
                    if !matches!(editor_state.modal_waiting, ModalWaitingState::None) {
                        // Modal已发送，不更新UI，等待Modal处理
                    } else {
                        editor_state.update_ui(&edit_interaction).await?;
                    }
                }
            }
            _ => {
                // 有等待中的Modal，同时等待Modal提交和新的按钮交互
                tokio::select! {
                    // 等待Modal提交
                    modal_result = response.await_modal_interaction(&serenity_ctx.shard) => {
                        if let Some(modal_interaction) = modal_result {
                            // 处理Modal提交
                            editor_state.handle_modal_submit(&modal_interaction).await?;
                            editor_state.modal_waiting = ModalWaitingState::None;

                            // 更新UI显示 - 使用原始interaction编辑响应
                            editor_state.update_ui(interaction).await?;
                        } else {
                            // Modal被取消，重置状态
                            editor_state.modal_waiting = ModalWaitingState::None;
                        }
                    }

                    // 等待新的按钮交互
                    button_result = response.await_component_interaction(&serenity_ctx.shard)
                        .author_id(interaction.user.id)
                        .timeout(std::time::Duration::from_secs(INTERACTION_TIMEOUT_SECS)) => {

                        if let Some(edit_interaction) = button_result {
                            // 新的按钮交互到达，放弃Modal等待
                            if !matches!(editor_state.modal_waiting, ModalWaitingState::None) {
                                tracing::info!("New button interaction received, abandoning modal wait");
                                editor_state.modal_waiting = ModalWaitingState::None;
                            }

                            // 处理按钮交互
                            let should_exit = editor_state.handle_interaction(&edit_interaction).await?;

                            if should_exit {
                                if edit_interaction.data.custom_id == "save_license" {
                                    editor_state.cleanup_ui(&edit_interaction).await?;
                                    return Ok(LicenseEditorOutcome {
                                        state: Some(editor_state.get_state().clone()),
                                        interaction: Some(edit_interaction.clone()),
                                    });
                                } else {
                                    editor_state.cleanup_ui(&edit_interaction).await?;
                                    return Ok(LicenseEditorOutcome {
                                        state: None,
                                        interaction: Some(edit_interaction.clone()),
                                    });
                                }
                            } else {
                                // 更新UI显示（如果不是Modal操作）
                                if matches!(editor_state.modal_waiting, ModalWaitingState::None) {
                                    editor_state.update_ui(&edit_interaction).await?;
                                }
                            }
                        } else {
                            // 超时，清理UI
                            editor_state.cleanup_ui(interaction).await?;
                            return Ok(LicenseEditorOutcome {
                                state: None,
                                interaction: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Modal等待状态
#[derive(Debug, Clone)]
pub enum ModalWaitingState {
    None,
    WaitingForName,
    WaitingForRestrictions,
}

/// 协议编辑器
pub struct LicenseEditor<'a> {
    serenity_ctx: &'a serenity::all::Context,
    core: EditorCore,
    modal_waiting: ModalWaitingState,
}

impl<'a> LicenseEditor<'a> {
    pub fn new(
        serenity_ctx: &'a serenity::all::Context,
        _data: &'a Data,
        state: LicenseEditState,
    ) -> Self {
        Self {
            serenity_ctx,
            core: EditorCore::new(state),
            modal_waiting: ModalWaitingState::None,
        }
    }

    pub fn get_state(&self) -> &LicenseEditState {
        self.core.get_state()
    }

    /// 处理Modal提交
    pub async fn handle_modal_submit(
        &mut self,
        modal_interaction: &ModalInteraction,
    ) -> Result<(), BotError> {
        // 确认Modal响应
        modal_interaction
            .create_response(
                &self.serenity_ctx.http,
                CreateInteractionResponse::Acknowledge,
            )
            .await?;

        // 根据等待状态处理不同类型的Modal
        match &self.modal_waiting {
            ModalWaitingState::WaitingForName => {
                // 处理名称编辑
                if let Some(ActionRowComponent::InputText(input)) = modal_interaction
                    .data
                    .components
                    .first()
                    .and_then(|row| row.components.first())
                {
                    let new_name = input.value.clone().unwrap_or_default();
                    self.core.get_state_mut().license_name = new_name;
                    tracing::info!(
                        "License name updated to: {}",
                        self.core.get_state().license_name
                    );
                }
            }
            ModalWaitingState::WaitingForRestrictions => {
                // 处理限制条件编辑
                if let Some(ActionRowComponent::InputText(input)) = modal_interaction
                    .data
                    .components
                    .first()
                    .and_then(|row| row.components.first())
                {
                    let value = input.value.clone().unwrap_or_default();
                    self.core.get_state_mut().restrictions_note = if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    };
                    tracing::info!(
                        "License restrictions updated to: {:?}",
                        self.core.get_state().restrictions_note
                    );
                }
            }
            ModalWaitingState::None => {
                warn!("Received modal submission but not waiting for any modal");
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl<'a> UIProvider for LicenseEditor<'a> {
    /// 确认交互
    async fn acknowledge(&self, interaction: &ComponentInteraction) -> Result<(), BotError> {
        interaction
            .create_response(self.serenity_ctx, CreateInteractionResponse::Acknowledge)
            .await?;
        Ok(())
    }

    /// 编辑响应，更新UI显示
    async fn edit_response(
        &self,
        interaction: &ComponentInteraction,
        embed: CreateEmbed,
        components: Vec<CreateActionRow>,
    ) -> Result<(), BotError> {
        interaction
            .edit_response(
                &self.serenity_ctx.http,
                EditInteractionResponse::new()
                    .embed(embed)
                    .components(components),
            )
            .await?;
        Ok(())
    }
}

impl<'a> LicenseEditor<'a> {
    /// 发送初始编辑界面
    pub async fn send_initial_ui(
        &self,
        interaction: &ComponentInteraction,
    ) -> Result<(), BotError> {
        let (embed, components) = self.core.build_ui();

        interaction
            .create_response(
                &self.serenity_ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("📝 **协议编辑器** - 点击按钮修改设置")
                        .embed(embed)
                        .components(components)
                        .ephemeral(true),
                ),
            )
            .await?;

        Ok(())
    }

    /// 更新编辑界面
    pub async fn update_ui(&self, interaction: &ComponentInteraction) -> Result<(), BotError> {
        let (embed, components) = self.core.build_ui();

        interaction
            .edit_response(
                &self.serenity_ctx.http,
                EditInteractionResponse::default()
                    .embed(embed)
                    .components(components),
            )
            .await?;

        Ok(())
    }

    /// 清理UI - 删除编辑器消息
    pub async fn cleanup_ui(&self, interaction: &ComponentInteraction) -> Result<(), BotError> {
        match interaction.delete_response(&self.serenity_ctx.http).await {
            Ok(()) => Ok(()),
            Err(err) => {
                if let serenity::Error::Http(http_err) = &err
                    && let serenity::http::HttpError::UnsuccessfulRequest(resp) = http_err
                {
                    let code = resp.error.code;
                    if code == 10062 || code == 10008 {
                        debug!(
                            error_code = code,
                            "Interaction response already gone while cleaning up editor"
                        );
                        return Ok(());
                    }
                }

                Err(err.into())
            }
        }
    }

    /// 处理用户交互
    pub async fn handle_interaction(
        &mut self,
        interaction: &ComponentInteraction,
    ) -> Result<bool, BotError> {
        match interaction.data.custom_id.as_str() {
            "edit_name" => {
                // 处理编辑名称 - 发送Modal但不等待结果
                let modal = CreateModal::new("edit_name_modal", "编辑协议名称").components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "协议名称", "name_input")
                            .placeholder("输入协议名称")
                            .value(&self.core.get_state().license_name)
                            .min_length(1)
                            .max_length(50)
                            .required(true),
                    ),
                ]);

                // 发送Modal
                interaction
                    .create_response(
                        &self.serenity_ctx.http,
                        CreateInteractionResponse::Modal(modal),
                    )
                    .await?;

                // 设置Modal等待状态
                self.modal_waiting = ModalWaitingState::WaitingForName;
                tracing::info!(
                    "Modal sent for name editing, waiting for submission or new interaction"
                );

                Ok(false) // 继续编辑，但现在处于Modal等待状态
            }
            "edit_restrictions" => {
                // 处理编辑限制条件 - 发送Modal但不等待结果
                let modal =
                    CreateModal::new("edit_restrictions_modal", "编辑限制条件").components(vec![
                        CreateActionRow::InputText(
                            CreateInputText::new(
                                InputTextStyle::Paragraph,
                                "限制条件",
                                "restrictions_input",
                            )
                            .placeholder("输入限制条件（可选）")
                            .value(
                                self.core
                                    .get_state()
                                    .restrictions_note
                                    .clone()
                                    .unwrap_or_default(),
                            )
                            .max_length(1000)
                            .required(false),
                        ),
                    ]);

                // 发送Modal
                interaction
                    .create_response(
                        &self.serenity_ctx.http,
                        CreateInteractionResponse::Modal(modal),
                    )
                    .await?;

                // 设置Modal等待状态
                self.modal_waiting = ModalWaitingState::WaitingForRestrictions;
                tracing::info!(
                    "Modal sent for restrictions editing, waiting for submission or new interaction"
                );

                Ok(false) // 继续编辑，但现在处于Modal等待状态
            }
            "toggle_redistribution" => {
                self.acknowledge(interaction).await?;
                self.core.get_state_mut().allow_redistribution =
                    !self.core.get_state().allow_redistribution;
                Ok(false) // 继续编辑
            }
            "toggle_modification" => {
                self.acknowledge(interaction).await?;
                self.core.get_state_mut().allow_modification =
                    !self.core.get_state().allow_modification;
                Ok(false) // 继续编辑
            }
            "toggle_backup" => {
                self.acknowledge(interaction).await?;
                self.core.get_state_mut().allow_backup = !self.core.get_state().allow_backup;
                Ok(false) // 继续编辑
            }
            "save_license" => {
                self.acknowledge(interaction).await?;
                Ok(true) // 保存并退出
            }
            "cancel_license" => {
                self.acknowledge(interaction).await?;
                Ok(true) // 取消并退出
            }
            _ => {
                warn!("Unknown interaction: {}", interaction.data.custom_id);
                Ok(false)
            }
        }
    }
}
