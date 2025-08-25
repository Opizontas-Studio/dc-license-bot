use prost::Message;
use sea_orm::{DatabaseConnection, EntityTrait, ActiveModelTrait, Set, IntoActiveModel};
use tracing::info;
use entities::user_settings;

// 包含生成的 protobuf 代码
pub mod license_management {
    tonic::include_proto!("license_management");
}
use license_management::*;

// 辅助函数：将 SeaORM 模型转换为 Protobuf 消息
fn to_proto_user_settings(model: user_settings::Model) -> UserSettings {
    UserSettings {
        user_id: model.user_id,
        auto_publish_enabled: model.auto_publish_enabled,
        skip_auto_publish_confirmation: model.skip_auto_publish_confirmation,
        default_user_license_id: model.default_user_license_id,
        default_system_license_name: model.default_system_license_name,
        default_system_license_backup: model.default_system_license_backup,
    }
}

pub async fn handle_get_user_settings(payload: &[u8], db: &DatabaseConnection) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let request = GetUserSettingsRequest::decode(payload)?;
    info!("Getting settings for user {}", request.user_id);

    let settings = user_settings::Entity::find_by_id(request.user_id)
        .one(db)
        .await?
        .ok_or_else(|| format!("Settings for user ID {} not found", request.user_id))?;

    let response = to_proto_user_settings(settings);
    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    Ok(buf)
}

pub async fn handle_update_user_settings(payload: &[u8], db: &DatabaseConnection) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let request = UpdateUserSettingsRequest::decode(payload)?;
    info!("Updating settings for user {}", request.user_id);

    let mut settings = user_settings::Entity::find_by_id(request.user_id)
        .one(db)
        .await?
        .map(|m| m.into_active_model())
        .unwrap_or_else(|| user_settings::ActiveModel {
            user_id: Set(request.user_id),
            ..Default::default()
        });

    if let Some(val) = request.auto_publish_enabled { settings.auto_publish_enabled = Set(val); }
    if let Some(val) = request.skip_auto_publish_confirmation { settings.skip_auto_publish_confirmation = Set(val); }
    if let Some(val) = request.default_user_license_id { settings.default_user_license_id = Set(Some(val)); }
    if let Some(val) = request.default_system_license_name { settings.default_system_license_name = Set(Some(val)); }
    if let Some(val) = request.default_system_license_backup { settings.default_system_license_backup = Set(Some(val)); }

    let result = settings.save(db).await?;
    let model = result.try_into()
        .map_err(|e| format!("Failed to convert saved settings to model: {:?}", e))?;
    let response = to_proto_user_settings(model);

    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    Ok(buf)
}