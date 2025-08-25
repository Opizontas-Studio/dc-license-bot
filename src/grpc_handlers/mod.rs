pub mod user_license_handler;
pub mod user_settings_handler;
pub mod system_handler;

use crate::services::gateway::registry::ForwardRequest;
use sea_orm::DatabaseConnection;
use tracing::{debug, error, info};
use crate::config::BotCfg;

// gRPC 方法路由器
pub async fn handle_grpc_request(
    request: &ForwardRequest,
    db: &DatabaseConnection,
    cfg: &BotCfg,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let method_path = &request.method_path;
    let payload = &request.payload;
    
    info!("Handling gRPC request: {} (length: {})", method_path, method_path.len());
    debug!("Method path bytes: {:?}", method_path.as_bytes());
    
    // 移除可能的前导斜杠
    let normalized_path = method_path.strip_prefix('/').unwrap_or(method_path);
    
    debug!("Normalized path: {}", normalized_path);
    
    match normalized_path {
        // 用户许可证管理
        "LicenseManagementService.license_management/CreateUserLicense" => {
            debug!("Matched CreateUserLicense");
            user_license_handler::handle_create_user_license(payload, db).await
        },
        "LicenseManagementService.license_management/GetUserLicenses" => {
            debug!("Matched GetUserLicenses");
            user_license_handler::handle_get_user_licenses(payload, db).await
        },
        "LicenseManagementService.license_management/UpdateUserLicense" => {
            debug!("Matched UpdateUserLicense");
            user_license_handler::handle_update_user_license(payload, db).await
        },
        "LicenseManagementService.license_management/DeleteUserLicense" => {
            debug!("Matched DeleteUserLicense");
            user_license_handler::handle_delete_user_license(payload, db).await
        },
        "LicenseManagementService.license_management/IncrementUsageCount" => {
            debug!("Matched IncrementUsageCount");
            user_license_handler::handle_increment_usage_count(payload, db).await
        },
        
        // 用户设置管理
        "LicenseManagementService.license_management/GetUserSettings" => {
            debug!("Matched GetUserSettings");
            user_settings_handler::handle_get_user_settings(payload, db).await
        },
        "LicenseManagementService.license_management/UpdateUserSettings" => {
            debug!("Matched UpdateUserSettings");
            user_settings_handler::handle_update_user_settings(payload, db).await
        },
        
        // 系统状态
        "LicenseManagementService.license_management/Ping" => {
            debug!("Matched Ping");
            system_handler::handle_ping(payload, cfg).await
        },
        
        _ => {
            error!("Unknown gRPC method: {}", method_path);
            Err(format!("Unknown method: {}", method_path).into())
        }
    }
}