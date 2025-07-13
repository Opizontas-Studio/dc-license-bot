use migration::{Migrator, MigratorTrait, SchemaManager};
use serenity::all::*;

#[cfg(test)]
use crate::database::BotDatabase;

async fn setup_test_db() -> BotDatabase {
    let db = BotDatabase::new_memory().await.unwrap();
    let migrations = Migrator::migrations();
    let manager = SchemaManager::new(db.inner());
    for migration in migrations {
        migration.up(&manager).await.unwrap();
    }
    db
}

#[tokio::test]
async fn test_create_license() {
    let db = setup_test_db().await;
    let service = db.license();
    let user_id = UserId::new(123);

    let license = service
        .create(
            user_id,
            "Test License".to_string(),
            true,
            false,
            Some("Test restrictions".to_string()),
            true,
        )
        .await
        .unwrap();

    assert_eq!(license.license_name, "Test License");
    assert!(license.allow_redistribution);
    assert!(!license.allow_modification);
    assert_eq!(
        license.restrictions_note,
        Some("Test restrictions".to_string())
    );
    assert!(license.allow_backup);
    assert_eq!(license.usage_count, 0);
}

#[tokio::test]
async fn test_get_user_licenses() {
    let db = setup_test_db().await;
    let service = db.license();
    let user_id = UserId::new(123);

    // Create two licenses
    service
        .create(user_id, "License 1".to_string(), true, true, None, false)
        .await
        .unwrap();

    service
        .create(
            user_id,
            "License 2".to_string(),
            false,
            false,
            Some("Restrictions".to_string()),
            true,
        )
        .await
        .unwrap();

    let licenses = service.get_user_licenses(user_id).await.unwrap();
    assert_eq!(licenses.len(), 2);

    // Should be ordered by created_at desc
    assert_eq!(licenses[0].license_name, "License 2");
    assert_eq!(licenses[1].license_name, "License 1");
}

#[tokio::test]
async fn test_update_license() {
    let db = setup_test_db().await;
    let service = db.license();
    let user_id = UserId::new(123);

    let license = service
        .create(user_id, "Original".to_string(), true, false, None, false)
        .await
        .unwrap();

    let updated = service
        .update(
            license.id,
            user_id,
            "Updated".to_string(),
            false,
            true,
            Some("New restrictions".to_string()),
            true,
        )
        .await
        .unwrap();

    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.license_name, "Updated");
    assert!(!updated.allow_redistribution);
    assert!(updated.allow_modification);
    assert_eq!(
        updated.restrictions_note,
        Some("New restrictions".to_string())
    );
    assert!(updated.allow_backup);
}

#[tokio::test]
async fn test_delete_license() {
    let db = setup_test_db().await;
    let service = db.license();
    let user_id = UserId::new(123);

    let license = service
        .create(user_id, "Test".to_string(), true, false, None, false)
        .await
        .unwrap();

    let deleted = service.delete(license.id, user_id).await.unwrap();
    assert!(deleted);

    let licenses = service.get_user_licenses(user_id).await.unwrap();
    assert_eq!(licenses.len(), 0);
}

#[tokio::test]
async fn test_increment_usage() {
    let db = setup_test_db().await;
    let service = db.license();
    let user_id = UserId::new(123);

    let license = service
        .create(user_id, "Test".to_string(), true, false, None, false)
        .await
        .unwrap();

    service.increment_usage(license.id, user_id).await.unwrap();
    service.increment_usage(license.id, user_id).await.unwrap();

    let updated_license = service
        .get_license(license.id, user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_license.usage_count, 2);
}

#[tokio::test]
async fn test_license_name_exists() {
    let db = setup_test_db().await;
    let service = db.license();
    let user_id = UserId::new(123);

    service
        .create(user_id, "Existing".to_string(), true, false, None, false)
        .await
        .unwrap();

    assert!(
        service
            .license_name_exists(user_id, "Existing", None)
            .await
            .unwrap()
    );
    assert!(
        !service
            .license_name_exists(user_id, "Non-existing", None)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_get_user_license_count() {
    let db = setup_test_db().await;
    let service = db.license();
    let user_id = UserId::new(123);

    assert_eq!(service.get_user_license_count(user_id).await.unwrap(), 0);

    service
        .create(user_id, "License 1".to_string(), true, false, None, false)
        .await
        .unwrap();

    assert_eq!(service.get_user_license_count(user_id).await.unwrap(), 1);
}
