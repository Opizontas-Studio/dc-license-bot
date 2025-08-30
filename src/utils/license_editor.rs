use serenity::all::*;
use tracing::warn;

use super::editor_core::{EditorCore, LicenseEditState, UIProvider};
use crate::{commands::Data, error::BotError};

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
/// - `Some(LicenseEditState)`: 用户保存的最终状态
/// - `None`: 用户取消了编辑
pub async fn present_license_editing_panel(
    serenity_ctx: &serenity::all::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    initial_state: LicenseEditState,
) -> Result<Option<LicenseEditState>, BotError> {
    // 创建编辑器状态
    let mut editor_state = LicenseEditor::new(serenity_ctx, data, initial_state);

    // 发送初始编辑界面
    editor_state.send_initial_ui(interaction).await?;

    // 主编辑循环
    loop {
        // 等待用户交互
        let Some(edit_interaction) = interaction
            .get_response(&serenity_ctx.http)
            .await?
            .await_component_interaction(&serenity_ctx.shard)
            .author_id(interaction.user.id)
            .timeout(std::time::Duration::from_secs(1800)) // 30分钟超时
            .await
        else {
            // 超时，清理UI
            editor_state.cleanup_ui(interaction).await?;
            return Ok(None);
        };

        // 处理交互
        let should_exit = editor_state.handle_interaction(&edit_interaction).await?;

        if should_exit {
            // 检查是否是保存操作
            if edit_interaction.data.custom_id == "save_license" {
                // 清理UI并返回最终状态
                editor_state.cleanup_ui(&edit_interaction).await?;
                return Ok(Some(editor_state.get_state().clone()));
            } else {
                // 取消操作
                editor_state.cleanup_ui(&edit_interaction).await?;
                return Ok(None);
            }
        } else {
            // 更新UI显示
            editor_state.update_ui(&edit_interaction).await?;
        }
    }
}

/// 协议编辑器
pub struct LicenseEditor<'a> {
    serenity_ctx: &'a serenity::all::Context,
    core: EditorCore,
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
        }
    }

    pub fn get_state(&self) -> &LicenseEditState {
        self.core.get_state()
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
        interaction
            .delete_response(&self.serenity_ctx.http)
            .await?;

        Ok(())
    }

    /// 处理用户交互
    pub async fn handle_interaction(
        &mut self,
        interaction: &ComponentInteraction,
    ) -> Result<bool, BotError> {
        match interaction.data.custom_id.as_str() {
            "edit_name" => {
                // 处理编辑名称 - 立即响应Modal，不等待结果
                // 在 custom_id 中编码消息ID以便后续更新
                let message_id = interaction.message.id;
                let modal_id = format!("edit_name_modal_{}", message_id);
                
                let modal = CreateModal::new(modal_id, "编辑协议名称").components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "协议名称", "name_input")
                            .placeholder("输入协议名称")
                            .value(&self.core.get_state().license_name)
                            .min_length(1)
                            .max_length(50)
                            .required(true),
                    ),
                ]);

                // 直接发送Modal响应，不等待结果
                interaction
                    .create_response(
                        &self.serenity_ctx.http,
                        CreateInteractionResponse::Modal(modal),
                    )
                    .await?;

                Ok(false) // 继续编辑，Modal处理将在全局事件处理器中异步进行
            }
            "edit_restrictions" => {
                // 处理编辑限制条件 - 立即响应Modal，不等待结果
                // 在 custom_id 中编码消息ID以便后续更新
                let message_id = interaction.message.id;
                let modal_id = format!("edit_restrictions_modal_{}", message_id);
                
                let modal =
                    CreateModal::new(modal_id, "编辑限制条件").components(vec![
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

                // 直接发送Modal响应，不等待结果
                interaction
                    .create_response(
                        &self.serenity_ctx.http,
                        CreateInteractionResponse::Modal(modal),
                    )
                    .await?;

                Ok(false) // 继续编辑
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
