use crate::{error::BotError, types::license::SystemLicense, utils::LicenseEmbedBuilder};
use serenity::all::*;

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

/// UI提供者trait，抽象不同框架的UI操作
#[async_trait::async_trait]
pub trait UIProvider {
    /// 展示Modal并返回Modal交互结果
    async fn present_modal(
        &self,
        interaction: &ComponentInteraction,
        modal: CreateModal,
    ) -> Result<Option<ModalInteraction>, BotError>;

    /// 确认交互
    async fn acknowledge(&self, interaction: &ComponentInteraction) -> Result<(), BotError>;

    /// 编辑响应，更新UI显示
    async fn edit_response(
        &self,
        interaction: &ComponentInteraction,
        embed: CreateEmbed,
        components: Vec<CreateActionRow>,
    ) -> Result<(), BotError>;
}

/// 协议编辑器核心逻辑
pub struct EditorCore {
    state: LicenseEditState,
}

impl EditorCore {
    /// 创建新的编辑器核心
    pub fn new(state: LicenseEditState) -> Self {
        Self { state }
    }

    /// 获取当前编辑状态
    pub fn get_state(&self) -> &LicenseEditState {
        &self.state
    }

    /// 获取当前编辑状态的可变引用
    pub fn get_state_mut(&mut self) -> &mut LicenseEditState {
        &mut self.state
    }

    /// 构建UI界面
    pub fn build_ui(&self) -> (CreateEmbed, Vec<CreateActionRow>) {
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
            .label(if self.state.allow_redistribution {
                "关闭二传"
            } else {
                "开启二传"
            })
            .style(if self.state.allow_redistribution {
                ButtonStyle::Success
            } else {
                ButtonStyle::Secondary
            });

        let toggle_modification_btn = CreateButton::new("toggle_modification")
            .label(if self.state.allow_modification {
                "关闭二改"
            } else {
                "开启二改"
            })
            .style(if self.state.allow_modification {
                ButtonStyle::Success
            } else {
                ButtonStyle::Secondary
            });

        let toggle_backup_btn = CreateButton::new("toggle_backup")
            .label(if self.state.allow_backup {
                "关闭备份"
            } else {
                "开启备份"
            })
            .style(if self.state.allow_backup {
                ButtonStyle::Success
            } else {
                ButtonStyle::Secondary
            });

        let save_btn = CreateButton::new("save_license")
            .label("保存")
            .style(ButtonStyle::Primary);

        let cancel_btn = CreateButton::new("cancel_license")
            .label("取消")
            .style(ButtonStyle::Danger);

        // 组装按钮行
        let row1 = CreateActionRow::Buttons(vec![edit_name_btn, edit_restrictions_btn]);
        let row2 = CreateActionRow::Buttons(vec![
            toggle_redistribution_btn,
            toggle_modification_btn,
            toggle_backup_btn,
        ]);
        let row3 = CreateActionRow::Buttons(vec![save_btn, cancel_btn]);

        (embed, vec![row1, row2, row3])
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
        assert_eq!(
            state.restrictions_note,
            Some("Some restrictions".to_string())
        );
        assert!(state.allow_backup);
    }

    #[test]
    fn test_editor_core_build_ui() {
        let state = LicenseEditState::new("Test License".to_string());
        let core = EditorCore::new(state);
        let (_embed, components) = core.build_ui();

        assert_eq!(components.len(), 3); // 3 rows of buttons
        // 验证embed已创建，无需检查内部字段
        // 因为CreateEmbed的字段可能是私有的
    }
}
