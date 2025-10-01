use chrono::Utc;
use entities::user_licenses;
use prost::Message;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set,
};
use tracing::{debug, info};

// 包含生成的 protobuf 代码
pub mod license_management {
    tonic::include_proto!("license_management");
}
use license_management::*;

// 辅助函数：将 SeaORM 模型转换为 Protobuf 消息
fn to_proto_user_license(model: user_licenses::Model) -> UserLicense {
    UserLicense {
        id: model.id,
        user_id: model.user_id,
        license_name: model.license_name,
        allow_redistribution: model.allow_redistribution,
        allow_modification: model.allow_modification,
        restrictions_note: model.restrictions_note,
        allow_backup: model.allow_backup,
        usage_count: model.usage_count,
        created_at: Some(prost_types::Timestamp {
            seconds: model.created_at.timestamp(),
            nanos: model.created_at.timestamp_subsec_nanos() as i32,
        }),
    }
}

pub async fn handle_create_user_license(
    payload: &[u8],
    db: &DatabaseConnection,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let request = CreateUserLicenseRequest::decode(payload)?;
    info!(
        "Creating license for user {}: {}",
        request.user_id, request.license_name
    );

    let new_license = user_licenses::ActiveModel {
        user_id: Set(request.user_id),
        license_name: Set(request.license_name),
        allow_redistribution: Set(request.allow_redistribution),
        allow_modification: Set(request.allow_modification),
        restrictions_note: Set(request.restrictions_note),
        allow_backup: Set(request.allow_backup),
        usage_count: Set(0),
        created_at: Set(Utc::now()),
        ..Default::default()
    };

    let result = new_license.insert(db).await?;
    let response = to_proto_user_license(result);

    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    info!("Successfully created license with ID: {}", response.id);
    Ok(buf)
}

pub async fn handle_get_user_licenses(
    payload: &[u8],
    db: &DatabaseConnection,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let request = GetUserLicensesRequest::decode(payload)?;
    info!("Getting licenses for user {}", request.user_id);

    let licenses = user_licenses::Entity::find()
        .filter(user_licenses::Column::UserId.eq(request.user_id))
        .all(db)
        .await?;

    info!(
        "Found {} licenses for user {}",
        licenses.len(),
        request.user_id
    );

    let response = GetUserLicensesResponse {
        licenses: licenses.into_iter().map(to_proto_user_license).collect(),
    };

    info!("Created response with {} licenses", response.licenses.len());

    let mut buf = Vec::new();
    response.encode(&mut buf)?;

    info!("Encoded response to {} bytes", buf.len());
    debug!("Response object: {:#?}", response);
    debug!("Encoded response bytes: {:?}", buf);
    info!("Returning response successfully");

    Ok(buf)
}

pub async fn handle_update_user_license(
    payload: &[u8],
    db: &DatabaseConnection,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let request = UpdateUserLicenseRequest::decode(payload)?;
    info!("Updating license {}", request.id);

    let mut license = user_licenses::Entity::find_by_id(request.id)
        .one(db)
        .await?
        .ok_or_else(|| format!("License with ID {} not found", request.id))?
        .into_active_model();

    if let Some(name) = request.license_name {
        license.license_name = Set(name);
    }
    if let Some(allow) = request.allow_redistribution {
        license.allow_redistribution = Set(allow);
    }
    if let Some(allow) = request.allow_modification {
        license.allow_modification = Set(allow);
    }
    if let Some(note) = request.restrictions_note {
        license.restrictions_note = Set(Some(note));
    }
    if let Some(allow) = request.allow_backup {
        license.allow_backup = Set(allow);
    }

    let result = license.update(db).await?;
    let response = to_proto_user_license(result);

    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    Ok(buf)
}

pub async fn handle_delete_user_license(
    payload: &[u8],
    db: &DatabaseConnection,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let request = DeleteUserLicenseRequest::decode(payload)?;
    info!("Deleting license {}", request.id);

    let res = user_licenses::Entity::delete_by_id(request.id)
        .exec(db)
        .await?;

    let (success, message) = if res.rows_affected == 1 {
        (true, "License deleted successfully".to_string())
    } else {
        (
            false,
            format!(
                "License with ID {} not found or could not be deleted",
                request.id
            ),
        )
    };

    let response = DeleteUserLicenseResponse { success, message };
    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    Ok(buf)
}

pub async fn handle_increment_usage_count(
    payload: &[u8],
    db: &DatabaseConnection,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let request = IncrementUsageRequest::decode(payload)?;
    info!("Incrementing usage count for license {}", request.id);

    let license = user_licenses::Entity::find_by_id(request.id)
        .one(db)
        .await?
        .ok_or_else(|| format!("License with ID {} not found", request.id))?;

    let mut active_license = license.into_active_model();
    let new_count = active_license.usage_count.as_ref() + 1;
    active_license.usage_count = Set(new_count);
    active_license.update(db).await?;

    let response = IncrementUsageResponse {
        new_usage_count: new_count,
    };
    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    Ok(buf)
}
