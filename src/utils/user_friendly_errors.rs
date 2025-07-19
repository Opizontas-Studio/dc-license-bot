use crate::error::BotError;
use tracing::warn;

/// 用户友好的错误消息映射
pub struct UserFriendlyErrorMapper;

impl UserFriendlyErrorMapper {
    /// 将系统错误转换为用户友好的错误消息
    pub fn map_error(error: &BotError) -> String {
        match error {
            BotError::GenericError { message, .. } => {
                // 对于GenericError，检查是否是已知的用户友好错误
                if Self::is_user_friendly_message(message) {
                    message.clone()
                } else {
                    // 对于其他GenericError，记录详细错误但返回通用消息
                    warn!("Generic error occurred: {}", message);
                    "操作失败，请稍后再试".to_string()
                }
            }
            BotError::DatabaseError { .. } => {
                warn!("Database error occurred: {}", error);
                "数据库连接出现问题，请稍后再试".to_string()
            }
            BotError::DiscordError { .. } => {
                warn!("Discord API error occurred: {}", error);
                "Discord服务暂时不可用，请稍后再试".to_string()
            }
            BotError::SerdeError { .. } => {
                warn!("Serialization error occurred: {}", error);
                "数据处理出现问题，请稍后再试".to_string()
            }
            BotError::ReqwestError { .. } => {
                warn!("Network error occurred: {}", error);
                "网络连接出现问题，请检查网络连接".to_string()
            }
            BotError::ConfigError { .. } => {
                warn!("Configuration error occurred: {}", error);
                "系统配置出现问题，请联系管理员".to_string()
            }
            BotError::IoError { .. } => {
                warn!("IO error occurred: {}", error);
                "文件操作出现问题，请稍后再试".to_string()
            }
            BotError::ValidationError { message, .. } => {
                // 验证错误通常是用户输入问题，可以直接显示
                message.clone()
            }
            BotError::NotFoundError { .. } => {
                "未找到相关内容".to_string()
            }
            BotError::AuthorizationError { .. } => {
                "您没有权限执行此操作".to_string()
            }
            BotError::RateLimitError { .. } => {
                "操作太频繁，请稍后再试".to_string()
            }
            BotError::TimeoutError { .. } => {
                "操作超时，请稍后再试".to_string()
            }
            _ => {
                // 对于其他未知错误，记录详细信息但返回通用消息
                warn!("Unknown error occurred: {}", error);
                "发生未知错误，请稍后再试".to_string()
            }
        }
    }

    /// 检查消息是否已经是用户友好的
    fn is_user_friendly_message(message: &str) -> bool {
        // 检查消息是否包含技术术语或路径
        let technical_indicators = [
            "error", "Error", "failed", "Failed", "panic", "Panic",
            "database", "Database", "sql", "SQL", "sqlite", "SQLite",
            "/", "\\", "src/", "target/", ".rs", ".toml",
            "thread", "Thread", "mutex", "Mutex", "channel", "Channel",
            "http", "Http", "HTTP", "https", "HTTPS", "tcp", "TCP",
            "deserialize", "serialize", "json", "JSON", "xml", "XML",
            "tokio", "async", "await", "future", "Future",
            "NoneError", "UnwrapError", "ParseError", "IoError",
        ];

        // 如果消息包含技术术语，则认为不是用户友好的
        !technical_indicators.iter().any(|&term| message.contains(term))
    }

    /// 为特定操作提供上下文相关的错误消息
    pub fn map_operation_error(operation: &str, error: &BotError) -> String {
        let base_message = Self::map_error(error);
        
        match operation {
            "create_license" => {
                match error {
                    BotError::ValidationError { .. } => base_message,
                    BotError::DatabaseError { .. } => "协议创建失败，请稍后再试".to_string(),
                    BotError::NotFoundError { .. } => "无法创建协议，请检查输入信息".to_string(),
                    _ => "协议创建失败，请稍后再试".to_string(),
                }
            }
            "update_license" => {
                match error {
                    BotError::NotFoundError { .. } => "找不到要更新的协议".to_string(),
                    BotError::AuthorizationError { .. } => "您没有权限修改此协议".to_string(),
                    BotError::ValidationError { .. } => base_message,
                    _ => "协议更新失败，请稍后再试".to_string(),
                }
            }
            "delete_license" => {
                match error {
                    BotError::NotFoundError { .. } => "找不到要删除的协议".to_string(),
                    BotError::AuthorizationError { .. } => "您没有权限删除此协议".to_string(),
                    _ => "协议删除失败，请稍后再试".to_string(),
                }
            }
            "publish_license" => {
                match error {
                    BotError::NotFoundError { .. } => "找不到要发布的协议".to_string(),
                    BotError::AuthorizationError { .. } => "您没有权限在此频道发布协议".to_string(),
                    BotError::DiscordError { .. } => "Discord消息发送失败，请稍后再试".to_string(),
                    _ => "协议发布失败，请稍后再试".to_string(),
                }
            }
            "reload_licenses" => {
                match error {
                    BotError::IoError { .. } => "协议文件读取失败，请检查文件是否存在".to_string(),
                    BotError::SerdeError { .. } => "协议文件格式错误，请检查文件格式".to_string(),
                    BotError::AuthorizationError { .. } => "您没有权限重载系统协议".to_string(),
                    _ => "协议重载失败，请稍后再试".to_string(),
                }
            }
            "backup_notification" => {
                match error {
                    BotError::ReqwestError { .. } => "备份通知发送失败，但协议已正常发布".to_string(),
                    BotError::TimeoutError { .. } => "备份通知发送超时，但协议已正常发布".to_string(),
                    _ => "备份通知发送失败，但协议已正常发布".to_string(),
                }
            }
            _ => base_message,
        }
    }

    /// 为常见的用户输入错误提供建议
    pub fn get_user_suggestion(error: &BotError) -> Option<String> {
        match error {
            BotError::ValidationError { message, .. } => {
                if message.contains("name") || message.contains("名称") {
                    Some("请确保协议名称不为空且长度在1-50个字符之间".to_string())
                } else if message.contains("length") || message.contains("长度") {
                    Some("请检查输入内容的长度是否符合要求".to_string())
                } else if message.contains("format") || message.contains("格式") {
                    Some("请检查输入内容的格式是否正确".to_string())
                } else {
                    None
                }
            }
            BotError::RateLimitError { .. } => {
                Some("请等待几秒后再试".to_string())
            }
            BotError::AuthorizationError { .. } => {
                Some("请联系管理员获取相应权限".to_string())
            }
            BotError::ReqwestError { .. } => {
                Some("请检查网络连接，或联系管理员".to_string())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BotError;
    use snafu::Location;

    #[test]
    fn test_user_friendly_message_detection() {
        // 技术术语应该被识别为非用户友好
        assert!(!UserFriendlyErrorMapper::is_user_friendly_message("Database error occurred"));
        assert!(!UserFriendlyErrorMapper::is_user_friendly_message("Failed to parse JSON"));
        assert!(!UserFriendlyErrorMapper::is_user_friendly_message("src/main.rs:42"));
        
        // 用户友好的消息应该被识别
        assert!(UserFriendlyErrorMapper::is_user_friendly_message("操作失败，请稍后再试"));
        assert!(UserFriendlyErrorMapper::is_user_friendly_message("您没有权限执行此操作"));
        assert!(UserFriendlyErrorMapper::is_user_friendly_message("协议名称不能为空"));
    }

    #[test]
    fn test_operation_error_mapping() {
        let db_error = BotError::DatabaseError {
            message: "Connection failed".to_string(),
            loc: Location::new("test", 0, 0),
        };
        
        let create_error = UserFriendlyErrorMapper::map_operation_error("create_license", &db_error);
        assert_eq!(create_error, "协议创建失败，请稍后再试");
        
        let update_error = UserFriendlyErrorMapper::map_operation_error("update_license", &db_error);
        assert_eq!(update_error, "协议更新失败，请稍后再试");
    }

    #[test]
    fn test_user_suggestion() {
        let validation_error = BotError::ValidationError {
            message: "name is required".to_string(),
            loc: Location::new("test", 0, 0),
        };
        
        let suggestion = UserFriendlyErrorMapper::get_user_suggestion(&validation_error);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("协议名称"));
    }
}