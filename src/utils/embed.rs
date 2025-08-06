use entities::user_licenses::Model as UserLicense;
use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter, Timestamp};

// 常用字符串常量
const PERMISSION_ALLOWED: &str = "✅ 允许";
const PERMISSION_DENIED: &str = "❌ 不允许";
const COMMERCIAL_USE_DENIED: &str = "❌ 社区不允许任何作品用于商业化";
const NO_RESTRICTIONS: &str = "无特殊限制";
const LICENSE_PROTECTION_TEXT: &str = "本作品内容受以下授权协议保护：";
const REDISTRIBUTION_FIELD: &str = "社区内二次传播";
const MODIFICATION_FIELD: &str = "社区内二次修改";
const BACKUP_FIELD: &str = "管理组备份";
const COMMERCIAL_FIELD: &str = "商业化使用";
const RESTRICTIONS_FIELD: &str = "限制条件";

/// 协议相关的嵌入消息构建工具
pub struct LicenseEmbedBuilder;

impl LicenseEmbedBuilder {
    /// 格式化权限值
    fn format_permission(allowed: bool) -> &'static str {
        if allowed {
            PERMISSION_ALLOWED
        } else {
            PERMISSION_DENIED
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
                REDISTRIBUTION_FIELD,
                Self::format_permission(allow_redistribution),
                true,
            )
            .field(
                MODIFICATION_FIELD,
                Self::format_permission(allow_modification),
                true,
            )
            .field(BACKUP_FIELD, Self::format_permission(allow_backup), true)
            .field(COMMERCIAL_FIELD, COMMERCIAL_USE_DENIED, true)
            .field(RESTRICTIONS_FIELD, restrictions_note.unwrap_or(NO_RESTRICTIONS), false)
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
            .description(LICENSE_PROTECTION_TEXT)
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
            .description(format!("协议 '{license_name}' 已成功删除。"))
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
            .title(format!("📜 授权协议: {name}"))
            .description(LICENSE_PROTECTION_TEXT)
            .colour(Colour::BLUE);

        Self::add_license_fields(embed, redis, modify, backup.unwrap_or(false), rest)
    }

    /// 创建协议发布成功embed
    pub fn create_license_published_embed(license_name: &str) -> CreateEmbed {
        CreateEmbed::new()
            .title("✅ 协议已发布")
            .description(format!("协议 '{license_name}' 已成功发布到当前帖子。"))
            .colour(Colour::DARK_GREEN)
    }

    /// 创建自动发布设置embed
    pub fn create_auto_publish_settings_embed(
        auto_copyright: bool,
        license_name: String,
        skip_confirmation: bool,
        is_system_license: bool,
        default_system_license_backup: Option<bool>,
    ) -> CreateEmbed {
        let status_icon = if auto_copyright { "🟢" } else { "🔴" };
        let status_text = if auto_copyright { "已启用" } else { "已禁用" };
        
        let mut embed = CreateEmbed::new()
            .title("⚙️ 自动发布设置")
            .description("管理您的自动协议发布配置")
            .field(
                "🤖 自动发布状态",
                format!("{status_icon} {status_text}"),
                true,
            )
            .field(
                "📜 默认协议",
                if license_name == "未设置" {
                    "❌ 未设置".to_string()
                } else {
                    format!("✅ {license_name}")
                },
                true,
            )
            .field(
                "⚡ 跳过确认",
                if skip_confirmation {
                    "✅ 已启用"
                } else {
                    "❌ 已禁用"
                },
                true,
            )
            .colour(if auto_copyright {
                Colour::from_rgb(76, 175, 80)  // Material Green
            } else {
                Colour::from_rgb(158, 158, 158)  // Material Grey
            })
            .footer(CreateEmbedFooter::new("💡 点击下方按钮修改设置"))
            .timestamp(Timestamp::now());
            
        // 如果使用系统协议，显示备份权限设置
        if is_system_license {
            let (backup_icon, backup_text) = match default_system_license_backup {
                None => ("🔄", "使用系统默认"),
                Some(true) => ("✅", "允许备份"),
                Some(false) => ("❌", "禁止备份"),
            };
            embed = embed.field(
                "💾 备份权限", 
                format!("{backup_icon} {backup_text}"), 
                true
            );
        }
        
        embed
    }

    /// 创建协议发布embed（用于实际发布的协议消息）
    pub fn create_license_embed(
        license: &UserLicense,
        backup_allowed: bool,
        display_name: &str,
    ) -> CreateEmbed {
        let embed = CreateEmbed::new()
            .title("📜 授权协议")
            .description(LICENSE_PROTECTION_TEXT)
            .colour(Colour::BLUE);

        Self::add_license_fields(
            embed,
            license.allow_redistribution,
            license.allow_modification,
            backup_allowed,
            license.restrictions_note.as_deref(),
        )
        .footer(CreateEmbedFooter::new(format!("作者: {display_name}")))
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
            .title(format!("⚠️ [已作废] {original_title}"))
            .description(format!(
                "**此协议已被新协议替换**\n\n{original_description}"
            ))
            .colour(Colour::from_rgb(128, 128, 128)); // 灰色表示已作废

        // 添加原有字段
        for (name, value, inline) in original_fields {
            embed = embed.field(name, value, *inline);
        }

        // 添加footer和时间戳
        if let Some(footer_text) = original_footer {
            embed = embed.footer(CreateEmbedFooter::new(format!("{footer_text} | 已作废")));
        }

        embed.timestamp(Timestamp::now())
    }

    /// 创建无协议embed
    pub fn create_no_license_embed() -> CreateEmbed {
        Self::create_license_manager_embed().field("无协议", "您还没有创建任何协议。", false)
    }

    /// 创建设置页面无协议embed
    pub fn create_settings_no_license_embed() -> CreateEmbed {
        CreateEmbed::new()
            .title("🔧 自动发布设置")
            .description("没有可用的协议。")
            .colour(serenity::all::colours::branding::YELLOW)
    }

    /// 创建自动发布预览embed
    pub fn create_auto_publish_preview_embed(
        license: &UserLicense,
        display_name: &str,
    ) -> CreateEmbed {
        let embed = CreateEmbed::new()
            .title("📜 准备发布协议")
            .description("检测到您启用了自动发布功能，是否要为此帖子发布以下协议？")
            .colour(Colour::GOLD);

        Self::add_license_fields(
            embed,
            license.allow_redistribution,
            license.allow_modification,
            license.allow_backup,
            license.restrictions_note.as_deref(),
        )
        .footer(CreateEmbedFooter::new(format!("作者: {display_name}")))
        .timestamp(Timestamp::now())
    }
}
