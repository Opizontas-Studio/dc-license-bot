use chrono::Utc;
use entities::published_posts::*;
use sea_orm::{QueryOrder, QuerySelect, Set, prelude::*};
use serenity::all::*;

use crate::{database::BotDatabase, error::BotError};

pub type PublishedPost = Model;

pub struct PublishedPostsService<'a>(&'a BotDatabase);

impl BotDatabase {
    /// Get a reference to the published posts service
    pub fn published_posts(&self) -> PublishedPostsService<'_> {
        PublishedPostsService(self)
    }
}

impl PublishedPostsService<'_> {
    /// Record a published post
    pub async fn record(
        &self,
        thread_id: ChannelId,
        message_id: MessageId,
        user_id: UserId,
        backup_allowed: bool,
    ) -> Result<PublishedPost, BotError> {
        let post = ActiveModel {
            thread_id: Set(thread_id.get() as i64),
            message_id: Set(message_id.get() as i64),
            user_id: Set(user_id.get() as i64),
            backup_allowed: Set(backup_allowed),
            updated_at: Set(Utc::now()),
        };

        let result = post.insert(self.0.inner()).await?;
        Ok(result)
    }

    /// Update an existing published post
    pub async fn update(
        &self,
        thread_id: ChannelId,
        message_id: MessageId,
        backup_allowed: bool,
    ) -> Result<Option<PublishedPost>, BotError> {
        let post = Entity::find()
            .filter(Column::ThreadId.eq(thread_id.get() as i64))
            .one(self.0.inner())
            .await?;

        if let Some(post) = post {
            let mut active_post: ActiveModel = post.into();
            active_post.message_id = Set(message_id.get() as i64);
            active_post.backup_allowed = Set(backup_allowed);
            active_post.updated_at = Set(Utc::now());

            let updated = active_post.update(self.0.inner()).await?;
            Ok(Some(updated))
        } else {
            Ok(None)
        }
    }

    /// Get published post by thread ID
    pub async fn get_by_thread(
        &self,
        thread_id: ChannelId,
    ) -> Result<Option<PublishedPost>, BotError> {
        Ok(Entity::find()
            .filter(Column::ThreadId.eq(thread_id.get() as i64))
            .one(self.0.inner())
            .await?)
    }

    /// Get published post by message ID
    pub async fn get_by_message(
        &self,
        message_id: MessageId,
    ) -> Result<Option<PublishedPost>, BotError> {
        Ok(Entity::find()
            .filter(Column::MessageId.eq(message_id.get() as i64))
            .one(self.0.inner())
            .await?)
    }

    /// Check if a thread has a published post
    pub async fn has_published_post(&self, thread_id: ChannelId) -> Result<bool, BotError> {
        Ok(self.get_by_thread(thread_id).await?.is_some())
    }

    /// Get all posts by user
    pub async fn get_user_posts(&self, user_id: UserId) -> Result<Vec<PublishedPost>, BotError> {
        Ok(Entity::find()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .order_by_desc(Column::UpdatedAt)
            .all(self.0.inner())
            .await?)
    }

    /// Get posts with backup allowed
    pub async fn get_backup_allowed_posts(&self) -> Result<Vec<PublishedPost>, BotError> {
        Ok(Entity::find()
            .filter(Column::BackupAllowed.eq(true))
            .order_by_desc(Column::UpdatedAt)
            .all(self.0.inner())
            .await?)
    }

    /// Get posts within a time range
    pub async fn get_posts_in_range(
        &self,
        from: chrono::DateTime<Utc>,
        to: chrono::DateTime<Utc>,
    ) -> Result<Vec<PublishedPost>, BotError> {
        Ok(Entity::find()
            .filter(Column::UpdatedAt.gte(from).and(Column::UpdatedAt.lt(to)))
            .order_by_desc(Column::UpdatedAt)
            .all(self.0.inner())
            .await?)
    }

    /// Get posts updated since a specific time
    pub async fn get_posts_since(
        &self,
        since: chrono::DateTime<Utc>,
    ) -> Result<Vec<PublishedPost>, BotError> {
        Ok(Entity::find()
            .filter(Column::UpdatedAt.gte(since))
            .order_by_desc(Column::UpdatedAt)
            .all(self.0.inner())
            .await?)
    }

    /// Update backup permission for a post
    pub async fn update_backup_permission(
        &self,
        thread_id: ChannelId,
        backup_allowed: bool,
    ) -> Result<Option<PublishedPost>, BotError> {
        let post = Entity::find()
            .filter(Column::ThreadId.eq(thread_id.get() as i64))
            .one(self.0.inner())
            .await?;

        if let Some(post) = post {
            let mut active_post: ActiveModel = post.into();
            active_post.backup_allowed = Set(backup_allowed);
            active_post.updated_at = Set(Utc::now());

            let updated = active_post.update(self.0.inner()).await?;
            Ok(Some(updated))
        } else {
            Ok(None)
        }
    }

    /// Delete a published post
    pub async fn delete(&self, thread_id: ChannelId) -> Result<bool, BotError> {
        let result = Entity::delete_many()
            .filter(Column::ThreadId.eq(thread_id.get() as i64))
            .exec(self.0.inner())
            .await?;

        Ok(result.rows_affected > 0)
    }

    /// Delete posts by user
    pub async fn delete_user_posts(&self, user_id: UserId) -> Result<u64, BotError> {
        let result = Entity::delete_many()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .exec(self.0.inner())
            .await?;

        Ok(result.rows_affected)
    }

    /// Get count of posts by user
    pub async fn get_user_post_count(&self, user_id: UserId) -> Result<u64, BotError> {
        Ok(Entity::find()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .count(self.0.inner())
            .await?)
    }

    /// Get count of posts with backup allowed
    pub async fn get_backup_allowed_count(&self) -> Result<u64, BotError> {
        Ok(Entity::find()
            .filter(Column::BackupAllowed.eq(true))
            .count(self.0.inner())
            .await?)
    }

    /// Get total posts count
    pub async fn get_total_count(&self) -> Result<u64, BotError> {
        Ok(Entity::find().count(self.0.inner()).await?)
    }

    /// Record or update a published post (upsert operation)
    pub async fn record_or_update(
        &self,
        thread_id: ChannelId,
        message_id: MessageId,
        user_id: UserId,
        backup_allowed: bool,
    ) -> Result<PublishedPost, BotError> {
        // Try to update existing post first
        if let Some(updated) = self.update(thread_id, message_id, backup_allowed).await? {
            Ok(updated)
        } else {
            // Create new post if doesn't exist
            self.record(thread_id, message_id, user_id, backup_allowed)
                .await
        }
    }

    /// Check if backup permission has changed for a thread
    pub async fn has_backup_permission_changed(
        &self,
        thread_id: ChannelId,
        new_backup_allowed: bool,
    ) -> Result<bool, BotError> {
        if let Some(post) = self.get_by_thread(thread_id).await? {
            Ok(post.backup_allowed != new_backup_allowed)
        } else {
            // If no existing post, consider it changed if backup is now allowed
            Ok(new_backup_allowed)
        }
    }

    /// Get recent posts (last N posts)
    pub async fn get_recent_posts(&self, limit: u64) -> Result<Vec<PublishedPost>, BotError> {
        Ok(Entity::find()
            .order_by_desc(Column::UpdatedAt)
            .limit(limit)
            .all(self.0.inner())
            .await?)
    }

    /// Clear all posts (dangerous operation)
    pub async fn clear_all(&self) -> Result<u64, BotError> {
        let result = Entity::delete_many().exec(self.0.inner()).await?;
        Ok(result.rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use migration::{Migrator, MigratorTrait, SchemaManager};

    use super::*;
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
    async fn test_record_post() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let user_id = UserId::new(789);

        let post = service
            .record(thread_id, message_id, user_id, true)
            .await
            .unwrap();

        assert_eq!(post.thread_id, 123);
        assert_eq!(post.message_id, 456);
        assert_eq!(post.user_id, 789);
        assert!(post.backup_allowed);
    }

    #[tokio::test]
    async fn test_get_by_thread() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let user_id = UserId::new(789);

        // Should be None initially
        assert!(service.get_by_thread(thread_id).await.unwrap().is_none());

        // Record post
        service
            .record(thread_id, message_id, user_id, true)
            .await
            .unwrap();

        // Should find the post
        let post = service.get_by_thread(thread_id).await.unwrap();
        assert!(post.is_some());
        let post = post.unwrap();
        assert_eq!(post.thread_id, 123);
    }

    #[tokio::test]
    async fn test_update_post() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let new_message_id = MessageId::new(999);
        let user_id = UserId::new(789);

        // Record initial post
        service
            .record(thread_id, message_id, user_id, true)
            .await
            .unwrap();

        // Update the post
        let updated = service
            .update(thread_id, new_message_id, false)
            .await
            .unwrap();

        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.message_id, 999);
        assert!(!updated.backup_allowed);
    }

    #[tokio::test]
    async fn test_has_published_post() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let user_id = UserId::new(789);

        // Initially false
        assert!(!service.has_published_post(thread_id).await.unwrap());

        // Record post
        service
            .record(thread_id, message_id, user_id, true)
            .await
            .unwrap();

        // Now true
        assert!(service.has_published_post(thread_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_user_posts() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let user_id = UserId::new(789);
        let other_user_id = UserId::new(999);

        // Record posts for different users
        service
            .record(ChannelId::new(123), MessageId::new(456), user_id, true)
            .await
            .unwrap();
        service
            .record(ChannelId::new(124), MessageId::new(457), user_id, false)
            .await
            .unwrap();
        service
            .record(
                ChannelId::new(125),
                MessageId::new(458),
                other_user_id,
                true,
            )
            .await
            .unwrap();

        let user_posts = service.get_user_posts(user_id).await.unwrap();
        assert_eq!(user_posts.len(), 2);

        let other_posts = service.get_user_posts(other_user_id).await.unwrap();
        assert_eq!(other_posts.len(), 1);
    }

    #[tokio::test]
    async fn test_get_backup_allowed_posts() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let user_id = UserId::new(789);

        // Record posts with different backup permissions
        service
            .record(ChannelId::new(123), MessageId::new(456), user_id, true)
            .await
            .unwrap();
        service
            .record(ChannelId::new(124), MessageId::new(457), user_id, false)
            .await
            .unwrap();
        service
            .record(ChannelId::new(125), MessageId::new(458), user_id, true)
            .await
            .unwrap();

        let backup_posts = service.get_backup_allowed_posts().await.unwrap();
        assert_eq!(backup_posts.len(), 2);
    }

    #[tokio::test]
    async fn test_update_backup_permission() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let user_id = UserId::new(789);

        // Record post with backup allowed
        service
            .record(thread_id, message_id, user_id, true)
            .await
            .unwrap();

        // Update backup permission
        let updated = service
            .update_backup_permission(thread_id, false)
            .await
            .unwrap();

        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert!(!updated.backup_allowed);
    }

    #[tokio::test]
    async fn test_has_backup_permission_changed() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let user_id = UserId::new(789);

        // For non-existing post, should return true if backup is allowed
        assert!(
            service
                .has_backup_permission_changed(thread_id, true)
                .await
                .unwrap()
        );
        assert!(
            !service
                .has_backup_permission_changed(thread_id, false)
                .await
                .unwrap()
        );

        // Record post with backup allowed
        service
            .record(thread_id, message_id, user_id, true)
            .await
            .unwrap();

        // Same permission should return false
        assert!(
            !service
                .has_backup_permission_changed(thread_id, true)
                .await
                .unwrap()
        );

        // Different permission should return true
        assert!(
            service
                .has_backup_permission_changed(thread_id, false)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_record_or_update() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let new_message_id = MessageId::new(999);
        let user_id = UserId::new(789);

        // First call should create
        let post1 = service
            .record_or_update(thread_id, message_id, user_id, true)
            .await
            .unwrap();
        assert_eq!(post1.message_id, 456);

        // Second call should update
        let post2 = service
            .record_or_update(thread_id, new_message_id, user_id, false)
            .await
            .unwrap();
        assert_eq!(post2.message_id, 999);
        assert!(!post2.backup_allowed);

        // Should only have one post
        assert_eq!(service.get_total_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_delete_post() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let thread_id = ChannelId::new(123);
        let message_id = MessageId::new(456);
        let user_id = UserId::new(789);

        // Record post
        service
            .record(thread_id, message_id, user_id, true)
            .await
            .unwrap();

        // Delete post
        let deleted = service.delete(thread_id).await.unwrap();
        assert!(deleted);

        // Should not exist anymore
        assert!(!service.has_published_post(thread_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_posts_in_range() {
        let db = setup_test_db().await;
        let service = db.published_posts();
        let user_id = UserId::new(789);
        let now = Utc::now();

        // Record a post
        service
            .record(ChannelId::new(123), MessageId::new(456), user_id, true)
            .await
            .unwrap();

        // Get posts in range
        let from = now - Duration::minutes(1);
        let to = now + Duration::minutes(1);
        let posts = service.get_posts_in_range(from, to).await.unwrap();
        assert_eq!(posts.len(), 1);

        // Get posts outside range
        let from_old = now - Duration::hours(2);
        let to_old = now - Duration::hours(1);
        let old_posts = service.get_posts_in_range(from_old, to_old).await.unwrap();
        assert_eq!(old_posts.len(), 0);
    }
}
