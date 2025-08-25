use prost::Message;
use tracing::info;
use chrono::Utc;
use crate::config::BotCfg;

// 包含生成的 protobuf 代码
pub mod license_management {
    tonic::include_proto!("license_management");
}
use license_management::*;

pub async fn handle_ping(payload: &[u8], cfg: &BotCfg) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let _request = PingRequest::decode(payload)?;
    info!("Ping request received");

    let uptime_seconds = (Utc::now() - cfg.bot_start_time).num_seconds();

    let response = PingResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds,
    };

    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    info!("Ping response sent");
    Ok(buf)
}