use std::{collections::HashMap, sync::OnceLock, time::Instant};

use serenity::all::{Context, GuildChannel};
use tokio::sync::RwLock;

use crate::{commands::Data, error::BotError};

use super::auto_publish_flow::AutoPublishFlow;

// 线程创建事件去重缓存，存储最近处理过的线程ID和处理时间
static PROCESSED_THREADS: OnceLock<RwLock<HashMap<u64, Instant>>> = OnceLock::new();

pub async fn handle_thread_create(
    ctx: &Context,
    thread: &GuildChannel,
    data: &Data,
) -> Result<(), BotError> {
    // 0. 去重检查 - 防止Discord事件重复触发
    let thread_id = thread.id.get();
    let now = Instant::now();

    {
        let cache = PROCESSED_THREADS.get_or_init(|| RwLock::new(HashMap::new()));
        let mut write_cache = cache.write().await;

        // 检查是否已处理过（5分钟内）
        if let Some(&processed_time) = write_cache.get(&thread_id) {
            if now.duration_since(processed_time).as_secs() < 300 {
                tracing::debug!(
                    "Thread {} already processed, skipping duplicate event",
                    thread_id
                );
                return Ok(());
            }
        }

        // 清理过期记录并标记当前线程
        write_cache.retain(|_, &mut time| now.duration_since(time).as_secs() < 300);
        write_cache.insert(thread_id, now);
    }

    // 等待1秒，确保帖子作者有时间发送第一条消息
    // Discord要求帖子作者必须先发送消息，机器人才能在线程中发送消息
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 1. 获取帖子创建者
    let Some(owner_id) = thread.owner_id else {
        return Ok(());
    };

    // 2. 使用新的状态机处理所有逻辑
    let flow = AutoPublishFlow::new(ctx, data, owner_id, thread);
    flow.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::BotDatabase;
    use crate::types::license::DefaultLicenseIdentifier;
    use crate::utils::LicenseEditState;
    use migration::{Migrator, MigratorTrait, SchemaManager};
    use serenity::all::UserId;

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
    async fn test_save_license_and_set_default() {
        let db = setup_test_db().await;
        let user_id = UserId::new(123);

        // 创建一个测试的编辑状态
        let edit_state = LicenseEditState::new("Test License".to_string());

        // 测试保存协议 - 直接测试数据库层面的逻辑
        let (name, allow_redistribution, allow_modification, restrictions_note, allow_backup) =
            edit_state.to_user_license_fields();

        // 创建协议
        let license = db
            .license()
            .create(
                user_id,
                name,
                allow_redistribution,
                allow_modification,
                restrictions_note,
                allow_backup,
            )
            .await
            .unwrap();

        assert_eq!(license.license_name, "Test License");
        assert_eq!(license.user_id, user_id.get() as i64);

        // 设置为默认协议
        db.user_settings()
            .set_default_license(
                user_id,
                Some(DefaultLicenseIdentifier::User(license.id)),
                None,
            )
            .await
            .unwrap();

        // 验证协议已设置为默认
        let settings = db.user_settings().get(user_id).await.unwrap().unwrap();
        assert_eq!(settings.default_user_license_id, Some(license.id));
    }

    #[tokio::test]
    async fn test_save_license_exceeds_limit() {
        let db = setup_test_db().await;
        let user_id = UserId::new(456);

        // 先创建5个协议（达到上限）
        for i in 0..5 {
            db.license()
                .create(user_id, format!("License {}", i), false, false, None, false)
                .await
                .unwrap();
        }

        // 验证协议数量已达到上限
        let count = db.license().get_user_license_count(user_id).await.unwrap();
        assert_eq!(count, 5);

        // 尝试创建第6个协议，应该失败
        let result = db
            .license()
            .create(user_id, "License 6".to_string(), false, false, None, false)
            .await;

        // 现在验证逻辑已经移到了 service 层，第6个协议应该被拒绝
        assert!(result.is_err());

        if let Err(BotError::GenericError { message, .. }) = result {
            assert!(message.contains("最多只能创建5个协议"));
        } else {
            panic!("Expected GenericError with correct message");
        }
    }

    #[tokio::test]
    async fn test_license_edit_state_conversion() {
        let edit_state = LicenseEditState::from_existing(
            "Test License".to_string(),
            true,
            false,
            Some("No commercial use".to_string()),
            true,
        );

        let (name, allow_redistribution, allow_modification, restrictions_note, allow_backup) =
            edit_state.to_user_license_fields();

        assert_eq!(name, "Test License");
        assert!(allow_redistribution);
        assert!(!allow_modification);
        assert_eq!(restrictions_note, Some("No commercial use".to_string()));
        assert!(allow_backup);
    }
}
