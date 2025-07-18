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
