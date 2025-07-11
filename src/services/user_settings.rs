use entities::user_settings::*;
use sea_orm::{prelude::*, Set};
use serenity::all::*;

use crate::{database::BotDatabase, error::BotError, types::license::DefaultLicenseIdentifier};

pub type UserSettings = Model;

pub struct UserSettingsService<'a>(&'a BotDatabase);

impl BotDatabase {
    /// Get a reference to the user settings service
    pub fn user_settings(&self) -> UserSettingsService<'_> {
        UserSettingsService(self)
    }
}

impl UserSettingsService<'_> {
    /// Get user settings, create default if not exists
    pub async fn get_or_create(&self, user_id: UserId) -> Result<UserSettings, BotError> {
        let user_id_i64 = user_id.get() as i64;
        
        if let Some(settings) = Entity::find()
            .filter(Column::UserId.eq(user_id_i64))
            .one(self.0.inner())
            .await?
        {
            Ok(settings)
        } else {
            // Create default settings
            let default_settings = ActiveModel {
                user_id: Set(user_id_i64),
                auto_publish_enabled: Set(false),
                default_user_license_id: Set(None),
                default_system_license_name: Set(None),
            };
            
            let created = default_settings.insert(self.0.inner()).await?;
            Ok(created)
        }
    }

    /// Get user settings (returns None if not exists)
    pub async fn get(&self, user_id: UserId) -> Result<Option<UserSettings>, BotError> {
        Ok(Entity::find()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .one(self.0.inner())
            .await?)
    }

    /// Update auto publish setting
    pub async fn set_auto_publish(
        &self,
        user_id: UserId,
        enabled: bool,
    ) -> Result<UserSettings, BotError> {
        let settings = self.get_or_create(user_id).await?;
        let mut active_settings: ActiveModel = settings.into();
        active_settings.auto_publish_enabled = Set(enabled);
        
        let updated = active_settings.update(self.0.inner()).await?;
        Ok(updated)
    }

    /// Set default license
    pub async fn set_default_license(
        &self,
        user_id: UserId,
        license: Option<DefaultLicenseIdentifier>,
    ) -> Result<UserSettings, BotError> {
        let settings = self.get_or_create(user_id).await?;
        let mut active_settings: ActiveModel = settings.into();
        
        match license {
            Some(DefaultLicenseIdentifier::User(id)) => {
                active_settings.default_user_license_id = Set(Some(id));
                active_settings.default_system_license_name = Set(None);
            }
            Some(DefaultLicenseIdentifier::System(name)) => {
                active_settings.default_user_license_id = Set(None);
                active_settings.default_system_license_name = Set(Some(name));
            }
            None => {
                active_settings.default_user_license_id = Set(None);
                active_settings.default_system_license_name = Set(None);
            }
        }
        
        let updated = active_settings.update(self.0.inner()).await?;
        Ok(updated)
    }

    /// Toggle auto publish setting
    pub async fn toggle_auto_publish(&self, user_id: UserId) -> Result<UserSettings, BotError> {
        let settings = self.get_or_create(user_id).await?;
        let new_enabled = !settings.auto_publish_enabled;
        
        let mut active_settings: ActiveModel = settings.into();
        active_settings.auto_publish_enabled = Set(new_enabled);
        
        let updated = active_settings.update(self.0.inner()).await?;
        Ok(updated)
    }

    /// Check if auto publish is enabled for user
    pub async fn is_auto_publish_enabled(&self, user_id: UserId) -> Result<bool, BotError> {
        let settings = self.get_or_create(user_id).await?;
        Ok(settings.auto_publish_enabled)
    }

    /// Get default license for user
    pub async fn get_default_license(&self, user_id: UserId) -> Result<Option<DefaultLicenseIdentifier>, BotError> {
        let settings = self.get_or_create(user_id).await?;
        
        if let Some(user_license_id) = settings.default_user_license_id {
            Ok(Some(DefaultLicenseIdentifier::User(user_license_id)))
        } else if let Some(system_license_name) = settings.default_system_license_name {
            Ok(Some(DefaultLicenseIdentifier::System(system_license_name)))
        } else {
            Ok(None)
        }
    }

    /// Clear default license (set to None)
    pub async fn clear_default_license(&self, user_id: UserId) -> Result<UserSettings, BotError> {
        self.set_default_license(user_id, None).await
    }

    /// Delete user settings
    pub async fn delete(&self, user_id: UserId) -> Result<bool, BotError> {
        let result = Entity::delete_many()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .exec(self.0.inner())
            .await?;
        
        Ok(result.rows_affected > 0)
    }

    /// Get all users with auto publish enabled
    pub async fn get_auto_publish_users(&self) -> Result<Vec<UserId>, BotError> {
        let settings = Entity::find()
            .filter(Column::AutoPublishEnabled.eq(true))
            .all(self.0.inner())
            .await?;
        
        Ok(settings
            .into_iter()
            .map(|s| UserId::new(s.user_id as u64))
            .collect())
    }

    /// Get count of users with auto publish enabled
    pub async fn get_auto_publish_count(&self) -> Result<u64, BotError> {
        Ok(Entity::find()
            .filter(Column::AutoPublishEnabled.eq(true))
            .count(self.0.inner())
            .await?)
    }

    /// Update settings with validation
    pub async fn update_settings(
        &self,
        user_id: UserId,
        auto_publish_enabled: Option<bool>,
        default_license: Option<Option<DefaultLicenseIdentifier>>,
    ) -> Result<UserSettings, BotError> {
        let settings = self.get_or_create(user_id).await?;
        let mut active_settings: ActiveModel = settings.into();
        
        if let Some(enabled) = auto_publish_enabled {
            active_settings.auto_publish_enabled = Set(enabled);
        }
        
        if let Some(license) = default_license {
            match license {
                Some(DefaultLicenseIdentifier::User(id)) => {
                    active_settings.default_user_license_id = Set(Some(id));
                    active_settings.default_system_license_name = Set(None);
                }
                Some(DefaultLicenseIdentifier::System(name)) => {
                    active_settings.default_user_license_id = Set(None);
                    active_settings.default_system_license_name = Set(Some(name));
                }
                None => {
                    active_settings.default_user_license_id = Set(None);
                    active_settings.default_system_license_name = Set(None);
                }
            }
        }
        
        let updated = active_settings.update(self.0.inner()).await?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::BotDatabase;
    use crate::types::license::DefaultLicenseIdentifier;
    use migration::{Migrator, MigratorTrait, SchemaManager};

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
    async fn test_get_or_create_settings() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user_id = UserId::new(123);

        // Should create default settings
        let settings = service.get_or_create(user_id).await.unwrap();
        assert_eq!(settings.user_id, 123);
        assert_eq!(settings.auto_publish_enabled, false);
        assert_eq!(settings.default_user_license_id, None);
        assert_eq!(settings.default_system_license_name, None);

        // Should return existing settings
        let settings2 = service.get_or_create(user_id).await.unwrap();
        assert_eq!(settings.user_id, settings2.user_id);
    }

    #[tokio::test]
    async fn test_set_auto_publish() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user_id = UserId::new(123);

        let settings = service.set_auto_publish(user_id, true).await.unwrap();
        assert_eq!(settings.auto_publish_enabled, true);

        let settings = service.set_auto_publish(user_id, false).await.unwrap();
        assert_eq!(settings.auto_publish_enabled, false);
    }

    #[tokio::test]
    async fn test_set_default_license() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user_id = UserId::new(123);

        // Test setting user license
        let settings = service.set_default_license(user_id, Some(DefaultLicenseIdentifier::User(42))).await.unwrap();
        assert_eq!(settings.default_user_license_id, Some(42));
        assert_eq!(settings.default_system_license_name, None);

        // Test setting system license
        let settings = service.set_default_license(user_id, Some(DefaultLicenseIdentifier::System("MIT".to_string()))).await.unwrap();
        assert_eq!(settings.default_user_license_id, None);
        assert_eq!(settings.default_system_license_name, Some("MIT".to_string()));

        // Test clearing license
        let settings = service.set_default_license(user_id, None).await.unwrap();
        assert_eq!(settings.default_user_license_id, None);
        assert_eq!(settings.default_system_license_name, None);
    }

    #[tokio::test]
    async fn test_toggle_auto_publish() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user_id = UserId::new(123);

        // Initially false
        let settings = service.get_or_create(user_id).await.unwrap();
        assert_eq!(settings.auto_publish_enabled, false);

        // Toggle to true
        let settings = service.toggle_auto_publish(user_id).await.unwrap();
        assert_eq!(settings.auto_publish_enabled, true);

        // Toggle back to false
        let settings = service.toggle_auto_publish(user_id).await.unwrap();
        assert_eq!(settings.auto_publish_enabled, false);
    }

    #[tokio::test]
    async fn test_is_auto_publish_enabled() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user_id = UserId::new(123);

        // Initially false
        assert_eq!(service.is_auto_publish_enabled(user_id).await.unwrap(), false);

        // Set to true
        service.set_auto_publish(user_id, true).await.unwrap();
        assert_eq!(service.is_auto_publish_enabled(user_id).await.unwrap(), true);
    }

    #[tokio::test]
    async fn test_get_default_license() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user_id = UserId::new(123);

        // Initially None
        assert_eq!(service.get_default_license(user_id).await.unwrap(), None);

        // Set to user license
        service.set_default_license(user_id, Some(DefaultLicenseIdentifier::User(42))).await.unwrap();
        assert_eq!(service.get_default_license(user_id).await.unwrap(), Some(DefaultLicenseIdentifier::User(42)));

        // Set to system license
        service.set_default_license(user_id, Some(DefaultLicenseIdentifier::System("Apache-2.0".to_string()))).await.unwrap();
        assert_eq!(service.get_default_license(user_id).await.unwrap(), Some(DefaultLicenseIdentifier::System("Apache-2.0".to_string())));
    }

    #[tokio::test]
    async fn test_delete_settings() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user_id = UserId::new(123);

        // Create settings
        service.get_or_create(user_id).await.unwrap();

        // Delete
        let deleted = service.delete(user_id).await.unwrap();
        assert!(deleted);

        // Should be None now
        let settings = service.get(user_id).await.unwrap();
        assert!(settings.is_none());
    }

    #[tokio::test]
    async fn test_get_auto_publish_users() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user1 = UserId::new(123);
        let user2 = UserId::new(456);
        let user3 = UserId::new(789);

        // Set up users
        service.set_auto_publish(user1, true).await.unwrap();
        service.set_auto_publish(user2, false).await.unwrap();
        service.set_auto_publish(user3, true).await.unwrap();

        let auto_publish_users = service.get_auto_publish_users().await.unwrap();
        assert_eq!(auto_publish_users.len(), 2);
        assert!(auto_publish_users.contains(&user1));
        assert!(auto_publish_users.contains(&user3));
        assert!(!auto_publish_users.contains(&user2));
    }

    #[tokio::test]
    async fn test_get_auto_publish_count() {
        let db = setup_test_db().await;
        let service = db.user_settings();
        let user1 = UserId::new(123);
        let user2 = UserId::new(456);

        // Initially 0
        assert_eq!(service.get_auto_publish_count().await.unwrap(), 0);

        // Add one user
        service.set_auto_publish(user1, true).await.unwrap();
        assert_eq!(service.get_auto_publish_count().await.unwrap(), 1);

        // Add another user
        service.set_auto_publish(user2, true).await.unwrap();
        assert_eq!(service.get_auto_publish_count().await.unwrap(), 2);

        // Disable one
        service.set_auto_publish(user1, false).await.unwrap();
        assert_eq!(service.get_auto_publish_count().await.unwrap(), 1);
    }
}