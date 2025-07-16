use poise::{Modal, ReplyHandle, CreateReply, execute_modal_on_component_interaction};
use serenity::all::*;
use tracing::warn;

use crate::{error::BotError, utils::LicenseEmbedBuilder, commands::{Context, Data}, types::license::SystemLicense};

/// 协议编辑状态，包含协议的所有可编辑字段
#[derive(Debug, Clone)]
pub struct LicenseEditState {
    pub license_name: String,
    pub allow_redistribution: bool,
    pub allow_modification: bool,
    pub restrictions_note: Option<String>,
    pub allow_backup: bool,
}

impl LicenseEditState {
    /// 创建新的协议编辑状态
    pub fn new(name: String) -> Self {
        Self {
            license_name: name,
            allow_redistribution: false,
            allow_modification: false,
            restrictions_note: None,
            allow_backup: false,
        }
    }

    /// 从现有协议创建编辑状态
    pub fn from_existing(
        name: String,
        allow_redistribution: bool,
        allow_modification: bool,
        restrictions_note: Option<String>,
        allow_backup: bool,
    ) -> Self {
        Self {
            license_name: name,
            allow_redistribution,
            allow_modification,
            restrictions_note,
            allow_backup,
        }
    }

    /// 从系统协议创建编辑状态
    pub fn from_system_license(system_license: &SystemLicense) -> Self {
        Self {
            license_name: system_license.license_name.clone(),
            allow_redistribution: system_license.allow_redistribution,
            allow_modification: system_license.allow_modification,
            restrictions_note: system_license.restrictions_note.clone(),
            allow_backup: system_license.allow_backup,
        }
    }

    /// 转换为用户协议的字段
    pub fn to_user_license_fields(&self) -> (String, bool, bool, Option<String>, bool) {
        (
            self.license_name.clone(),
            self.allow_redistribution,
            self.allow_modification,
            self.restrictions_note.clone(),
            self.allow_backup,
        )
    }
}

/// 协议编辑器，管理协议编辑的UI和交互
pub struct LicenseEditor<'a> {
    ctx: Context<'a>,
    state: LicenseEditState,
    reply_handle: ReplyHandle<'a>,
}

/// 编辑名称的Modal
#[derive(Modal)]
#[name = "编辑协议名称"]
struct EditNameModal {
    #[name = "协议名称"]
    #[placeholder = "输入协议名称"]
    #[min_length = 1]
    #[max_length = 100]
    name: String,
}

/// 编辑限制条件的Modal
#[derive(Modal)]
#[name = "编辑限制条件"]
struct EditRestrictionsModal {
    #[name = "限制条件"]
    #[placeholder = "输入限制条件（可选）"]
    #[max_length = 1000]
    restrictions: String,
}

impl<'a> LicenseEditor<'a> {
    /// 创建新的协议编辑器实例
    pub fn new(ctx: Context<'a>, state: LicenseEditState, reply_handle: ReplyHandle<'a>) -> Self {
        Self {
            ctx,
            state,
            reply_handle,
        }
    }

    /// 构建UI界面
    pub fn build_ui(&self) -> CreateReply {
        // 创建协议预览嵌入
        let embed = LicenseEmbedBuilder::create_license_preview_embed(
            &self.state.license_name,
            self.state.allow_redistribution,
            self.state.allow_modification,
            self.state.restrictions_note.as_deref(),
            Some(self.state.allow_backup),
        );

        // 创建按钮
        let edit_name_btn = CreateButton::new("edit_name")
            .label("编辑名称")
            .style(ButtonStyle::Secondary);

        let edit_restrictions_btn = CreateButton::new("edit_restrictions")
            .label("编辑限制条件")
            .style(ButtonStyle::Secondary);

        let toggle_redistribution_btn = CreateButton::new("toggle_redistribution")
            .label(if self.state.allow_redistribution { "关闭二传" } else { "开启二传" })
            .style(if self.state.allow_redistribution { ButtonStyle::Success } else { ButtonStyle::Secondary });

        let toggle_modification_btn = CreateButton::new("toggle_modification")
            .label(if self.state.allow_modification { "关闭二改" } else { "开启二改" })
            .style(if self.state.allow_modification { ButtonStyle::Success } else { ButtonStyle::Secondary });

        let toggle_backup_btn = CreateButton::new("toggle_backup")
            .label(if self.state.allow_backup { "关闭备份" } else { "开启备份" })
            .style(if self.state.allow_backup { ButtonStyle::Success } else { ButtonStyle::Secondary });

        let save_btn = CreateButton::new("save")
            .label("保存")
            .style(ButtonStyle::Primary);

        let cancel_btn = CreateButton::new("cancel")
            .label("取消")
            .style(ButtonStyle::Danger);

        // 组装按钮行
        let row1 = CreateActionRow::Buttons(vec![edit_name_btn, edit_restrictions_btn]);
        let row2 = CreateActionRow::Buttons(vec![toggle_redistribution_btn, toggle_modification_btn, toggle_backup_btn]);
        let row3 = CreateActionRow::Buttons(vec![save_btn, cancel_btn]);

        CreateReply::default()
            .embed(embed)
            .components(vec![row1, row2, row3])
    }

    /// 处理用户交互
    pub async fn handle_interaction(&mut self, interaction: &ComponentInteraction) -> Result<bool, BotError> {
        match interaction.data.custom_id.as_str() {
            "edit_name" => {
                // 处理编辑名称
                let defaults = EditNameModal {
                    name: self.state.license_name.clone(),
                };

                let Some(modal_resp) = execute_modal_on_component_interaction(
                    self.ctx,
                    interaction.clone(),
                    Some(defaults),
                    None,
                ).await? else {
                    warn!("Modal response is None for edit_name");
                    return Ok(false);
                };

                self.state.license_name = modal_resp.name;
                Ok(false) // 继续编辑
            }
            "edit_restrictions" => {
                // 处理编辑限制条件
                let defaults = EditRestrictionsModal {
                    restrictions: self.state.restrictions_note.clone().unwrap_or_default(),
                };

                let Some(modal_resp) = execute_modal_on_component_interaction(
                    self.ctx,
                    interaction.clone(),
                    Some(defaults),
                    None,
                ).await? else {
                    warn!("Modal response is None for edit_restrictions");
                    return Ok(false);
                };

                self.state.restrictions_note = if modal_resp.restrictions.trim().is_empty() {
                    None
                } else {
                    Some(modal_resp.restrictions)
                };
                Ok(false) // 继续编辑
            }
            "toggle_redistribution" => {
                interaction.create_response(&self.ctx, CreateInteractionResponse::Acknowledge).await?;
                self.state.allow_redistribution = !self.state.allow_redistribution;
                Ok(false) // 继续编辑
            }
            "toggle_modification" => {
                interaction.create_response(&self.ctx, CreateInteractionResponse::Acknowledge).await?;
                self.state.allow_modification = !self.state.allow_modification;
                Ok(false) // 继续编辑
            }
            "toggle_backup" => {
                interaction.create_response(&self.ctx, CreateInteractionResponse::Acknowledge).await?;
                self.state.allow_backup = !self.state.allow_backup;
                Ok(false) // 继续编辑
            }
            "save" => {
                interaction.create_response(&self.ctx, CreateInteractionResponse::Acknowledge).await?;
                Ok(true) // 保存并退出
            }
            "cancel" => {
                interaction.create_response(&self.ctx, CreateInteractionResponse::Acknowledge).await?;
                Ok(true) // 取消并退出
            }
            _ => {
                warn!("Unknown interaction: {}", interaction.data.custom_id);
                Ok(false)
            }
        }
    }

    /// 获取当前编辑状态
    pub fn get_state(&self) -> &LicenseEditState {
        &self.state
    }
}

/// 主要的协议编辑面板函数
/// 
/// 这个函数是模块的入口点，接收初始状态并返回最终状态
/// 
/// # 参数
/// - `ctx`: Poise 上下文
/// - `initial_state`: 初始的协议编辑状态
/// 
/// # 返回值
/// - `Some(LicenseEditState)`: 用户保存的最终状态
/// - `None`: 用户取消了编辑
pub async fn present_license_editing_panel(
    ctx: Context<'_>,
    initial_state: LicenseEditState,
) -> Result<Option<LicenseEditState>, BotError> {
    // 发送初始UI
    let reply = ctx.send(CreateReply::default().content("正在加载协议编辑器...")).await?;
    
    // 创建编辑器实例
    let mut editor = LicenseEditor::new(ctx, initial_state, reply);
    
    // 更新UI显示
    editor.reply_handle.edit(ctx, editor.build_ui()).await?;
    
    // 主交互循环
    loop {
        // 等待用户交互
        let Some(interaction) = editor.reply_handle
            .message()
            .await?
            .await_component_interaction(ctx)
            .author_id(ctx.author().id)
            .timeout(std::time::Duration::from_secs(300)) // 5分钟超时
            .await
        else {
            // 超时，清理UI
            editor.reply_handle.edit(ctx, CreateReply::default()
                .content("编辑器已超时，请重新开始。")
                .components(vec![])
            ).await?;
            return Ok(None);
        };

        // 处理交互
        let should_exit = editor.handle_interaction(&interaction).await?;
        
        if should_exit {
            // 检查是否是保存操作
            if interaction.data.custom_id == "save" {
                // 清理UI并返回最终状态
                editor.reply_handle.edit(ctx, CreateReply::default()
                    .content("协议已保存！")
                    .components(vec![])
                ).await?;
                return Ok(Some(editor.get_state().clone()));
            } else {
                // 取消操作
                editor.reply_handle.edit(ctx, CreateReply::default()
                    .content("已取消编辑。")
                    .components(vec![])
                ).await?;
                return Ok(None);
            }
        } else {
            // 更新UI显示
            editor.reply_handle.edit(ctx, editor.build_ui()).await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_license_edit_state_new() {
        let state = LicenseEditState::new("Test License".to_string());
        assert_eq!(state.license_name, "Test License");
        assert!(!state.allow_redistribution);
        assert!(!state.allow_modification);
        assert!(state.restrictions_note.is_none());
        assert!(!state.allow_backup);
    }

    #[test]
    fn test_license_edit_state_from_existing() {
        let state = LicenseEditState::from_existing(
            "Existing License".to_string(),
            true,
            false,
            Some("Some restrictions".to_string()),
            true,
        );
        assert_eq!(state.license_name, "Existing License");
        assert!(state.allow_redistribution);
        assert!(!state.allow_modification);
        assert_eq!(state.restrictions_note, Some("Some restrictions".to_string()));
        assert!(state.allow_backup);
    }
}

/// 使用 Serenity Context 的协议编辑面板
/// 
/// 这个函数为非 Poise 环境提供完整的协议编辑功能
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
pub async fn present_license_editing_panel_with_serenity_context(
    serenity_ctx: &serenity::all::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    initial_state: LicenseEditState,
) -> Result<Option<LicenseEditState>, BotError> {
    // 创建编辑器状态
    let mut editor_state = SerenityLicenseEditor::new(serenity_ctx, data, initial_state);
    
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
            .timeout(std::time::Duration::from_secs(300)) // 5分钟超时
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

/// 使用 Serenity Context 的协议编辑器
pub struct SerenityLicenseEditor<'a> {
    serenity_ctx: &'a serenity::all::Context,
    data: &'a Data,
    state: LicenseEditState,
}

impl<'a> SerenityLicenseEditor<'a> {
    pub fn new(serenity_ctx: &'a serenity::all::Context, data: &'a Data, state: LicenseEditState) -> Self {
        Self {
            serenity_ctx,
            data,
            state,
        }
    }
    
    pub fn get_state(&self) -> &LicenseEditState {
        &self.state
    }
    
    /// 发送初始编辑界面
    pub async fn send_initial_ui(&self, interaction: &ComponentInteraction) -> Result<(), BotError> {
        let reply = self.build_ui();
        
        interaction
            .create_response(
                &self.serenity_ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(reply.embeds.into_iter().next().unwrap())
                        .components(reply.components.unwrap_or_default())
                        .ephemeral(true),
                ),
            )
            .await?;
        
        Ok(())
    }
    
    /// 更新编辑界面
    pub async fn update_ui(&self, interaction: &ComponentInteraction) -> Result<(), BotError> {
        let reply = self.build_ui();
        
        interaction
            .edit_response(
                &self.serenity_ctx.http,
                EditInteractionResponse::default()
                    .embed(reply.embeds.into_iter().next().unwrap())
                    .components(reply.components.unwrap_or_default()),
            )
            .await?;
        
        Ok(())
    }
    
    /// 清理UI
    pub async fn cleanup_ui(&self, interaction: &ComponentInteraction) -> Result<(), BotError> {
        interaction
            .edit_response(
                &self.serenity_ctx.http,
                EditInteractionResponse::default()
                    .content("编辑器已关闭。")
                    .embeds(Vec::new())
                    .components(Vec::new()),
            )
            .await?;
        
        Ok(())
    }
    
    /// 构建UI界面
    pub fn build_ui(&self) -> CreateReply {
        // 创建协议预览嵌入
        let embed = LicenseEmbedBuilder::create_license_preview_embed(
            &self.state.license_name,
            self.state.allow_redistribution,
            self.state.allow_modification,
            self.state.restrictions_note.as_deref(),
            Some(self.state.allow_backup),
        );

        // 创建按钮
        let edit_name_btn = CreateButton::new("edit_name")
            .label("编辑名称")
            .style(ButtonStyle::Secondary);

        let edit_restrictions_btn = CreateButton::new("edit_restrictions")
            .label("编辑限制条件")
            .style(ButtonStyle::Secondary);

        let toggle_redistribution_btn = CreateButton::new("toggle_redistribution")
            .label(if self.state.allow_redistribution { "关闭二传" } else { "开启二传" })
            .style(if self.state.allow_redistribution { ButtonStyle::Success } else { ButtonStyle::Secondary });

        let toggle_modification_btn = CreateButton::new("toggle_modification")
            .label(if self.state.allow_modification { "关闭二改" } else { "开启二改" })
            .style(if self.state.allow_modification { ButtonStyle::Success } else { ButtonStyle::Secondary });

        let toggle_backup_btn = CreateButton::new("toggle_backup")
            .label(if self.state.allow_backup { "关闭备份" } else { "开启备份" })
            .style(if self.state.allow_backup { ButtonStyle::Success } else { ButtonStyle::Secondary });

        let save_btn = CreateButton::new("save_license")
            .label("保存")
            .style(ButtonStyle::Primary);

        let cancel_btn = CreateButton::new("cancel_license")
            .label("取消")
            .style(ButtonStyle::Danger);

        // 组装按钮行
        let row1 = CreateActionRow::Buttons(vec![edit_name_btn, edit_restrictions_btn]);
        let row2 = CreateActionRow::Buttons(vec![toggle_redistribution_btn, toggle_modification_btn, toggle_backup_btn]);
        let row3 = CreateActionRow::Buttons(vec![save_btn, cancel_btn]);

        CreateReply::default()
            .embed(embed)
            .components(vec![row1, row2, row3])
    }
    
    /// 处理用户交互
    pub async fn handle_interaction(&mut self, interaction: &ComponentInteraction) -> Result<bool, BotError> {
        match interaction.data.custom_id.as_str() {
            "edit_name" => {
                // 处理编辑名称 - 使用 serenity 的 Modal 处理
                let modal = CreateModal::new("edit_name_modal", "编辑协议名称")
                    .components(vec![
                        CreateActionRow::InputText(
                            CreateInputText::new(
                                InputTextStyle::Short,
                                "协议名称",
                                "name_input",
                            )
                            .placeholder("输入协议名称")
                            .value(&self.state.license_name)
                            .min_length(1)
                            .max_length(100)
                            .required(true),
                        )
                    ]);

                interaction.create_response(
                    &self.serenity_ctx.http,
                    CreateInteractionResponse::Modal(modal),
                ).await?;

                // 等待 Modal 提交
                let Some(modal_interaction) = interaction
                    .get_response(&self.serenity_ctx.http)
                    .await?
                    .await_modal_interaction(&self.serenity_ctx.shard)
                    .timeout(std::time::Duration::from_secs(300))
                    .await
                else {
                    warn!("Modal interaction timeout for edit_name");
                    return Ok(false);
                };

                // 确认响应
                modal_interaction.create_response(
                    &self.serenity_ctx.http,
                    CreateInteractionResponse::Acknowledge,
                ).await?;

                // 提取输入值
                if let Some(ActionRowComponent::InputText(input)) = modal_interaction.data.components
                    .get(0).and_then(|row| row.components.get(0))
                {
                    self.state.license_name = input.value.clone().unwrap_or_default();
                }

                Ok(false) // 继续编辑
            }
            "edit_restrictions" => {
                // 处理编辑限制条件 - 使用 serenity 的 Modal 处理
                let modal = CreateModal::new("edit_restrictions_modal", "编辑限制条件")
                    .components(vec![
                        CreateActionRow::InputText(
                            CreateInputText::new(
                                InputTextStyle::Paragraph,
                                "限制条件",
                                "restrictions_input",
                            )
                            .placeholder("输入限制条件（可选）")
                            .value(&self.state.restrictions_note.clone().unwrap_or_default())
                            .max_length(1000)
                            .required(false),
                        )
                    ]);

                interaction.create_response(
                    &self.serenity_ctx.http,
                    CreateInteractionResponse::Modal(modal),
                ).await?;

                // 等待 Modal 提交
                let Some(modal_interaction) = interaction
                    .get_response(&self.serenity_ctx.http)
                    .await?
                    .await_modal_interaction(&self.serenity_ctx.shard)
                    .timeout(std::time::Duration::from_secs(300))
                    .await
                else {
                    warn!("Modal interaction timeout for edit_restrictions");
                    return Ok(false);
                };

                // 确认响应
                modal_interaction.create_response(
                    &self.serenity_ctx.http,
                    CreateInteractionResponse::Acknowledge,
                ).await?;

                // 提取输入值
                if let Some(ActionRowComponent::InputText(input)) = modal_interaction.data.components
                    .get(0).and_then(|row| row.components.get(0))
                {
                    let value = input.value.clone().unwrap_or_default();
                    self.state.restrictions_note = if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    };
                }

                Ok(false) // 继续编辑
            }
            "toggle_redistribution" => {
                interaction.create_response(self.serenity_ctx, CreateInteractionResponse::Acknowledge).await?;
                self.state.allow_redistribution = !self.state.allow_redistribution;
                Ok(false) // 继续编辑
            }
            "toggle_modification" => {
                interaction.create_response(self.serenity_ctx, CreateInteractionResponse::Acknowledge).await?;
                self.state.allow_modification = !self.state.allow_modification;
                Ok(false) // 继续编辑
            }
            "toggle_backup" => {
                interaction.create_response(self.serenity_ctx, CreateInteractionResponse::Acknowledge).await?;
                self.state.allow_backup = !self.state.allow_backup;
                Ok(false) // 继续编辑
            }
            "save_license" => {
                interaction.create_response(self.serenity_ctx, CreateInteractionResponse::Acknowledge).await?;
                Ok(true) // 保存并退出
            }
            "cancel_license" => {
                interaction.create_response(self.serenity_ctx, CreateInteractionResponse::Acknowledge).await?;
                Ok(true) // 取消并退出
            }
            _ => {
                warn!("Unknown interaction: {}", interaction.data.custom_id);
                Ok(false)
            }
        }
    }
}