use entities::user_licenses;
use prost::Message;
use sea_orm::{DatabaseConnection, EntityTrait};
use serenity::all::UserId;
use std::io;
use tracing::{debug, info};

// 包含生成的 protobuf 代码
pub mod license_management {
    tonic::include_proto!("license_management");
}
use license_management::*;

use crate::services::license::LicenseService;

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

    let CreateUserLicenseRequest {
        user_id,
        license_name,
        allow_redistribution,
        allow_modification,
        restrictions_note,
        allow_backup,
    } = request;

    let service = LicenseService::new(db);
    let user_id = UserId::new(user_id as u64);

    let result = match service
        .create(
            user_id,
            license_name,
            allow_redistribution,
            allow_modification,
            restrictions_note,
            allow_backup,
        )
        .await
    {
        Ok(model) => model,
        Err(e) => return Err(Box::new(e)),
    };

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

    let service = LicenseService::new(db);
    let user_id = UserId::new(request.user_id as u64);

    let licenses = match service.get_user_licenses(user_id).await {
        Ok(models) => models,
        Err(e) => return Err(Box::new(e)),
    };

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

    let service = LicenseService::new(db);

    let existing = user_licenses::Entity::find_by_id(request.id)
        .one(db)
        .await?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("License with ID {} not found", request.id),
            )
        })?;

    let user_id = UserId::new(existing.user_id as u64);

    let new_name = request
        .license_name
        .unwrap_or_else(|| existing.license_name.clone());
    let new_allow_redistribution = request
        .allow_redistribution
        .unwrap_or(existing.allow_redistribution);
    let new_allow_modification = request
        .allow_modification
        .unwrap_or(existing.allow_modification);
    let new_restrictions_note = match request.restrictions_note {
        Some(note) => Some(note),
        None => existing.restrictions_note.clone(),
    };
    let new_allow_backup = request.allow_backup.unwrap_or(existing.allow_backup);

    let updated = match service
        .update(
            request.id,
            user_id,
            new_name,
            new_allow_redistribution,
            new_allow_modification,
            new_restrictions_note,
            new_allow_backup,
        )
        .await
    {
        Ok(Some(model)) => model,
        Ok(None) => {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::NotFound,
                format!("License with ID {} not found", request.id),
            )));
        }
        Err(e) => return Err(Box::new(e)),
    };

    let response = to_proto_user_license(updated);

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

    let existing = user_licenses::Entity::find_by_id(request.id)
        .one(db)
        .await?;

    let (success, message) = if let Some(model) = existing {
        let service = LicenseService::new(db);
        let user_id = UserId::new(model.user_id as u64);
        match service.delete(request.id, user_id).await {
            Ok(true) => (true, "License deleted successfully".to_string()),
            Ok(false) => (
                false,
                format!(
                    "License with ID {} not found or could not be deleted",
                    request.id
                ),
            ),
            Err(e) => return Err(Box::new(e)),
        }
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
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("License with ID {} not found", request.id),
            )
        })?;

    let service = LicenseService::new(db);
    let user_id = UserId::new(license.user_id as u64);
    let new_count = license.usage_count + 1;

    if let Err(e) = service.increment_usage(request.id, user_id).await {
        return Err(Box::new(e));
    }

    let response = IncrementUsageResponse {
        new_usage_count: new_count,
    };
    let mut buf = Vec::new();
    response.encode(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::BotDatabase;
    use crate::services::license::LicenseService;
    use migration::{Migrator, MigratorTrait, SchemaManager};
    use serenity::all::UserId;

    async fn setup_db() -> BotDatabase {
        let db = BotDatabase::new_memory().await.unwrap();
        let manager = SchemaManager::new(db.inner());
        for migration in Migrator::migrations() {
            migration.up(&manager).await.unwrap();
        }
        db
    }

    #[tokio::test]
    async fn test_handle_create_user_license_success() {
        let db = setup_db().await;
        let conn = db.inner();

        let request = CreateUserLicenseRequest {
            user_id: 123,
            license_name: "Test License".to_string(),
            allow_redistribution: true,
            allow_modification: false,
            restrictions_note: Some("No commercial use".to_string()),
            allow_backup: false,
        };

        let mut payload = Vec::new();
        request.encode(&mut payload).unwrap();

        let response_bytes = handle_create_user_license(&payload, conn)
            .await
            .expect("handler should succeed");

        let response = UserLicense::decode(&*response_bytes).unwrap();
        assert_eq!(response.user_id, 123);
        assert_eq!(response.license_name, "Test License");
        assert!(response.allow_redistribution);
        assert!(!response.allow_modification);
        assert_eq!(
            response.restrictions_note,
            Some("No commercial use".to_string())
        );
    }

    #[tokio::test]
    async fn test_handle_create_user_license_respects_limit() {
        let db = setup_db().await;
        let conn = db.inner();
        let service = LicenseService::new(conn);
        let user_id = UserId::new(456);

        for i in 0..5 {
            service
                .create(user_id, format!("License {i}"), false, false, None, false)
                .await
                .unwrap();
        }

        let overflow_request = CreateUserLicenseRequest {
            user_id: 456,
            license_name: "Overflow".to_string(),
            allow_redistribution: false,
            allow_modification: false,
            restrictions_note: None,
            allow_backup: false,
        };

        let mut payload = Vec::new();
        overflow_request.encode(&mut payload).unwrap();

        let err = handle_create_user_license(&payload, conn)
            .await
            .expect_err("handler should enforce license limit");

        assert!(
            err.to_string().contains("最多只能创建5个协议"),
            "unexpected error: {}",
            err
        );
    }
}
