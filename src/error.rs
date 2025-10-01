use snafu::{Location, Snafu};

#[derive(Snafu, Debug)]
pub enum BotError {
    #[snafu(display("验证失败: {}", message))]
    ValidationError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(transparent)]
    SerdeJsonError {
        #[snafu(implicit)]
        loc: Location,
        source: serde_json::Error,
    },
    #[snafu(transparent)]
    ParseIntError {
        #[snafu(implicit)]
        loc: Location,
        source: std::num::ParseIntError,
    },
    #[snafu(transparent)]
    JemallocCtlError {
        #[snafu(implicit)]
        loc: Location,
        source: tikv_jemalloc_ctl::Error,
    },
    #[snafu(transparent)]
    SeaOrmError {
        #[snafu(implicit)]
        loc: Location,
        source: sea_orm::DbErr,
    },
    #[snafu(transparent)]
    IoError {
        #[snafu(implicit)]
        loc: Location,
        source: std::io::Error,
    },
    #[snafu(transparent)]
    SerenityError {
        #[snafu(implicit)]
        loc: Location,
        #[snafu(source(from(serenity::Error, Box::new)))]
        source: Box<serenity::Error>,
    },
    #[snafu(display("数据库错误: {}", message))]
    DatabaseError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("Discord API错误: {}", message))]
    DiscordError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("网络请求错误: {}", message))]
    ReqwestError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("配置错误: {}", message))]
    ConfigError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("序列化错误: {}", message))]
    SerdeError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("未找到: {}", message))]
    NotFoundError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("权限不足: {}", message))]
    AuthorizationError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("操作频率限制: {}", message))]
    RateLimitError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(display("操作超时: {}", message))]
    TimeoutError {
        message: String,
        #[snafu(implicit)]
        loc: Location,
    },
    #[snafu(whatever, display("{message}"))]
    GenericError {
        message: String,
        // Having a `source` is optional, but if it is present, it must
        // have this specific attribute and type:
        #[snafu(source(from(Box<dyn std::error::Error + Send + Sync>, Some)))]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl BotError {
    /// 返回用户友好的错误消息
    pub fn user_message(&self) -> String {
        match self {
            BotError::ValidationError { message, .. } => message.clone(),
            BotError::DatabaseError { .. } => "数据库连接出现问题，请稍后再试".to_string(),
            BotError::DiscordError { .. } => "Discord服务暂时不可用，请稍后再试".to_string(),
            BotError::SerdeError { .. } => "数据处理出现问题，请稍后再试".to_string(),
            BotError::ReqwestError { .. } => "网络连接出现问题，请检查网络连接".to_string(),
            BotError::ConfigError { .. } => "系统配置出现问题，请联系管理员".to_string(),
            BotError::IoError { .. } => "文件操作出现问题，请稍后再试".to_string(),
            BotError::NotFoundError { .. } => "未找到相关内容".to_string(),
            BotError::AuthorizationError { .. } => "您没有权限执行此操作".to_string(),
            BotError::RateLimitError { .. } => "操作太频繁，请稍后再试".to_string(),
            BotError::TimeoutError { .. } => "操作超时，请稍后再试".to_string(),
            BotError::GenericError { .. } => "操作失败，请稍后再试".to_string(),
            _ => "发生未知错误，请稍后再试".to_string(),
        }
    }

    /// 返回针对特定操作的错误消息
    pub fn operation_message(&self, operation: &str) -> String {
        match (operation, self) {
            ("reload_licenses", BotError::IoError { .. }) => {
                "协议文件读取失败，请检查文件是否存在".to_string()
            }
            ("reload_licenses", BotError::SerdeError { .. }) => {
                "协议文件格式错误，请检查文件格式".to_string()
            }
            _ => self.user_message(),
        }
    }

    /// 返回用户建议
    pub fn user_suggestion(&self) -> Option<String> {
        match self {
            BotError::RateLimitError { .. } => Some("请等待几秒后再试".to_string()),
            BotError::AuthorizationError { .. } => Some("请联系管理员获取相应权限".to_string()),
            BotError::ReqwestError { .. } => Some("请检查网络连接，或联系管理员".to_string()),
            _ => None,
        }
    }
}
