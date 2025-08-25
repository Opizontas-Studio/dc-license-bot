use std::{sync::OnceLock, time::Duration};

use moka::future::Cache;
use serenity::all::{Context, GuildChannel};

use crate::{commands::Data, error::BotError};

use super::auto_publish_flow::AutoPublishFlow;

// 线程创建事件去重缓存，使用moka实现TTL自动清理
static PROCESSED_THREADS: OnceLock<Cache<u64, ()>> = OnceLock::new();

/// 检查线程中是否已有首条消息
/// Discord的ThreadCreate事件会在帖子创建和首条消息发送时都触发
/// 我们只想处理用户已发送首条消息的情况
async fn has_first_message(http: &serenity::all::Http, thread: &GuildChannel) -> bool {
    match thread.messages(http, serenity::all::GetMessages::new().limit(1)).await {
        Ok(messages) => !messages.is_empty(),
        Err(e) => {
            tracing::warn!("检查首条消息时出错: {}", e);
            false
        }
    }
}

pub async fn handle_thread_create(
    ctx: &Context,
    thread: &GuildChannel,
    data: &Data,
) -> Result<(), BotError> {
    // 0. 去重检查 - 防止Discord事件重复触发，使用TTL缓存自动清理
    let thread_id = thread.id.get();
    
    let cache = PROCESSED_THREADS.get_or_init(|| {
        Cache::builder()
            .time_to_live(Duration::from_secs(300))  // 5分钟TTL
            .max_capacity(10_000)                    // 限制最大条目数
            .build()
    });

    // 检查是否已处理过
    if cache.get(&thread_id).await.is_some() {
        tracing::debug!(
            "Thread {} already processed, skipping duplicate event",
            thread_id
        );
        return Ok(());
    }

    // 标记当前线程已处理（TTL会自动清理过期条目）
    cache.insert(thread_id, ()).await;

    // 额外检查：确保论坛频道在白名单中（双重检查，防止竞态条件）
    if let Some(parent_id) = thread.parent_id {
        let cfg = data.cfg().load();
        let is_allowed = cfg.allowed_forum_channels.is_empty() 
            || cfg.allowed_forum_channels.contains(&parent_id);
        
        if !is_allowed {
            tracing::debug!(
                "Thread {} created in non-allowed forum (parent: {}), skipping auto publish",
                thread_id,
                parent_id
            );
            return Ok(());
        }
    }

    // 检查这是否是真正的帖子创建（用户已发首条消息）
    // Discord傻逼设计：帖子创建和首条消息发送都会触发ThreadCreate事件
    // 我们只处理用户已发送首条消息的事件
    if !has_first_message(&ctx.http, thread).await {
        tracing::debug!(
            "ThreadCreate事件触发但用户尚未发送首条消息，跳过处理 (thread: {})",
            thread_id
        );
        return Ok(());
    }

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
