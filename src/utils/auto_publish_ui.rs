use serenity::all::*;
use crate::services::license::UserLicense;
use crate::utils::LicenseEmbedBuilder;

/// 自动发布流程的UI构建器
pub struct AutoPublishUI;

impl AutoPublishUI {
    /// 构建新用户引导消息
    pub fn build_guidance_message() -> CreateMessage {
        CreateMessage::new()
            .content("你好！我们发现你发了一个新帖子。你是否想开启'自动添加许可协议'的功能呢？")
            .components(vec![CreateActionRow::Buttons(vec![
                CreateButton::new("enable_auto_publish_setup")
                    .label("启用")
                    .style(ButtonStyle::Success),
                CreateButton::new("disable_auto_publish_setup")
                    .label("关闭")
                    .style(ButtonStyle::Danger),
            ])])
    }

    /// 构建协议选择菜单
    pub fn build_license_selection_menu(
        system_licenses: &[crate::types::license::SystemLicense],
    ) -> CreateSelectMenu {
        let mut select_options = vec![
            CreateSelectMenuOption::new("创建新协议", "new_license")
                .description("创建一个全新的协议"),
        ];

        for license in system_licenses {
            select_options.push(
                CreateSelectMenuOption::new(
                    &license.license_name,
                    format!("system_{}", license.license_name),
                )
                .description("基于系统协议创建"),
            );
        }

        CreateSelectMenu::new(
            "license_selection",
            CreateSelectMenuKind::String {
                options: select_options,
            },
        )
        .placeholder("请选择协议类型")
        .max_values(1)
    }

    /// 构建自动发布确认面板
    pub fn build_auto_publish_confirmation(
        license: &UserLicense,
        display_name: &str,
    ) -> CreateMessage {
        let embed = LicenseEmbedBuilder::create_auto_publish_preview_embed(license, display_name);

        CreateMessage::new()
            .embed(embed)
            .components(vec![CreateActionRow::Buttons(vec![
                CreateButton::new("confirm_auto_publish")
                    .label("✅ 确认发布")
                    .style(ButtonStyle::Success),
                CreateButton::new("cancel_auto_publish")
                    .label("❌ 取消")
                    .style(ButtonStyle::Danger),
            ])])
    }

    /// 构建发布确认按钮
    pub fn build_publish_confirmation_button() -> CreateButton {
        CreateButton::new("confirm_new_user_publish")
            .label("✅ 确认发布")
            .style(ButtonStyle::Success)
    }

    /// 创建启用功能的回复消息
    pub fn create_enable_response(select_menu: CreateSelectMenu) -> CreateInteractionResponseMessage {
        CreateInteractionResponseMessage::new()
            .content("✅ 自动发布功能已启用！\n\n请选择你要使用的协议：")
            .components(vec![CreateActionRow::SelectMenu(select_menu)])
            .ephemeral(true)
    }

    /// 创建关闭功能的回复消息
    pub fn create_disable_response() -> CreateInteractionResponseMessage {
        CreateInteractionResponseMessage::new()
            .content("好的，如果你改变主意，可以随时使用 `/自动发布设置` 手动开启。")
            .ephemeral(true)
    }

    /// 创建取消编辑的回复消息
    pub fn create_cancel_edit_response() -> CreateInteractionResponseFollowup {
        CreateInteractionResponseFollowup::new()
            .content("已取消协议创建。自动发布功能已启用，但您需要手动设置默认协议。")
            .ephemeral(true)
    }

    /// 创建发布取消的回复消息
    pub fn create_publish_cancel_response() -> CreateInteractionResponseMessage {
        CreateInteractionResponseMessage::new()
            .content("❌ 已取消发布")
            .ephemeral(true)
    }

    /// 创建新用户发布确认消息
    pub fn create_new_user_publish_confirmation(
        license: &UserLicense,
        display_name: &str,
    ) -> CreateInteractionResponseFollowup {
        let embed = LicenseEmbedBuilder::create_auto_publish_preview_embed(license, display_name);

        CreateInteractionResponseFollowup::new()
            .content("✅ 协议创建成功！\n\n📝 现在请确认是否要将其发布到这个帖子中：")
            .embed(embed)
            .components(vec![CreateActionRow::Buttons(vec![
                Self::build_publish_confirmation_button(),
            ])])
            .ephemeral(true)
    }

    /// 创建发布成功的编辑消息
    pub fn create_publish_success_edit() -> EditMessage {
        EditMessage::new()
            .content("协议已创建并设置为默认协议！自动发布功能现在已完全启用。")
            .components(Vec::new())
    }

    /// 创建新协议发布确认的followup消息
    pub fn create_new_license_publish_confirmation(license_name: &str) -> CreateInteractionResponseFollowup {
        let confirm_message = format!(
            "✅ 协议「{}」已创建并设置为默认协议！\n\n是否要在当前帖子中发布此协议？",
            license_name
        );

        CreateInteractionResponseFollowup::new()
            .content(confirm_message)
            .components(vec![CreateActionRow::Buttons(vec![
                CreateButton::new("confirm_publish_new_license")
                    .label("是的，发布")
                    .style(ButtonStyle::Success),
                CreateButton::new("skip_publish_new_license")
                    .label("暂不发布")
                    .style(ButtonStyle::Secondary),
            ])])
            .ephemeral(true)
    }
}