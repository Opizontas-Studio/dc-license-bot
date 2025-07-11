use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter, Timestamp};
use entities::user_licenses::Model as UserLicense;

/// 协议相关的嵌入消息构建工具
pub struct LicenseEmbedBuilder;

impl LicenseEmbedBuilder {
    /// 格式化权限值
    fn format_permission(allowed: bool) -> &'static str {
        if allowed {
            "✅ 允许"
        } else {
            "❌ 不允许"
        }
    }

    /// 添加协议权限字段到embed
    fn add_license_fields(
        embed: CreateEmbed,
        allow_redistribution: bool,
        allow_modification: bool,
        allow_backup: bool,
        restrictions_note: Option<&str>,
    ) -> CreateEmbed {
        embed
            .field(
                "允许二次传播",
                Self::format_permission(allow_redistribution),
                true,
            )
            .field(
                "允许二次修改",
                Self::format_permission(allow_modification),
                true,
            )
            .field(
                "允许备份",
                Self::format_permission(allow_backup),
                true,
            )
            .field(
                "限制条件",
                restrictions_note.unwrap_or("无特殊限制"),
                false,
            )
    }
    /// 创建协议管理主菜单embed
    pub fn create_license_manager_embed() -> CreateEmbed {
        CreateEmbed::new()
            .title("📜 协议管理")
            .description("选择您要管理的协议：")
            .colour(Colour::DARK_BLUE)
    }

    /// 创建协议详情展示embed
    pub fn create_license_detail_embed(license: &UserLicense) -> CreateEmbed {
        let embed = CreateEmbed::new()
            .title(format!("📜 授权协议: {}", license.license_name))
            .description("本作品内容受以下授权协议保护：")
            .colour(Colour::BLUE);
        
        Self::add_license_fields(
            embed,
            license.allow_redistribution,
            license.allow_modification,
            license.allow_backup,
            license.restrictions_note.as_deref(),
        )
    }

    /// 创建协议删除成功embed
    pub fn create_license_deleted_embed(license_name: &str) -> CreateEmbed {
        CreateEmbed::new()
            .title("✅ 协议已删除")
            .description(format!("协议 '{}' 已成功删除。", license_name))
            .colour(serenity::all::colours::branding::GREEN)
    }

    /// 创建协议预览embed
    pub fn create_license_preview_embed(
        name: &str,
        redis: bool,
        modify: bool,
        rest: Option<&str>,
        backup: Option<bool>,
    ) -> CreateEmbed {
        let embed = CreateEmbed::new()
            .title(format!("📜 授权协议: {}", name))
            .description("本作品内容受以下授权协议保护：")
            .colour(Colour::BLUE);
        
        Self::add_license_fields(
            embed,
            redis,
            modify,
            backup.unwrap_or(false),
            rest,
        )
    }

    /// 创建协议发布成功embed
    pub fn create_license_published_embed(license_name: &str) -> CreateEmbed {
        CreateEmbed::new()
            .title("✅ 协议已发布")
            .description(format!("协议 '{}' 已成功发布到当前帖子。", license_name))
            .colour(Colour::DARK_GREEN)
    }

    /// 创建自动发布设置embed
    pub fn create_auto_publish_settings_embed(
        auto_copyright: bool,
        license_name: String,
    ) -> CreateEmbed {
        CreateEmbed::new()
            .title("🔧 自动发布设置")
            .description("以下是自动发布的设置选项：")
            .field(
                "自动发布",
                if auto_copyright { "启用" } else { "禁用" },
                true,
            )
            .field("默认协议", license_name, true)
            .colour(if auto_copyright {
                serenity::all::colours::branding::GREEN
            } else {
                serenity::all::colours::branding::RED
            })
    }

    /// 创建协议发布embed（用于实际发布的协议消息）
    pub fn create_license_embed(
        license: &UserLicense,
        backup_allowed: bool,
        display_name: &str,
    ) -> CreateEmbed {
        let embed = CreateEmbed::new()
            .title("📜 授权协议")
            .description("本作品内容受以下授权协议保护：")
            .colour(Colour::BLUE);
        
        Self::add_license_fields(
            embed,
            license.allow_redistribution,
            license.allow_modification,
            backup_allowed,
            license.restrictions_note.as_deref(),
        )
        .footer(CreateEmbedFooter::new(format!("作者: {}", display_name)))
        .timestamp(Timestamp::now())
    }

    /// 创建作废协议embed
    pub fn create_obsolete_license_embed(
        original_title: &str,
        original_description: &str,
        original_fields: &[(String, String, bool)],
        original_footer: Option<&str>,
    ) -> CreateEmbed {
        let mut embed = CreateEmbed::new()
            .title(format!("⚠️ [已作废] {}", original_title))
            .description(format!(
                "**此协议已被新协议替换**\n\n{}",
                original_description
            ))
            .colour(Colour::from_rgb(128, 128, 128)); // 灰色表示已作废

        // 添加原有字段
        for (name, value, inline) in original_fields {
            embed = embed.field(name, value, *inline);
        }

        // 添加footer
        if let Some(footer_text) = original_footer {
            embed = embed.footer(CreateEmbedFooter::new(format!(
                "{} | 作废于 {}",
                footer_text,
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            )));
        }

        embed
    }

    /// 创建无协议embed
    pub fn create_no_license_embed() -> CreateEmbed {
        Self::create_license_manager_embed()
            .field("无协议", "您还没有创建任何协议。", false)
    }

    /// 创建设置页面无协议embed
    pub fn create_settings_no_license_embed() -> CreateEmbed {
        CreateEmbed::new()
            .title("🔧 自动发布设置")
            .description("没有可用的协议。")
            .colour(serenity::all::colours::branding::YELLOW)
    }
}