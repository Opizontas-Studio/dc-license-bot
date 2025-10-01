use serenity::all::{ChannelId, CreateMessage, EditMessage, GuildChannel, Http, MessageId, User};
use tracing::{error, info};

use crate::{
    commands::Data, error::BotError, services::notification_service::NotificationPayload,
    utils::LicenseEmbedBuilder,
};

pub struct LicensePublishService;

impl LicensePublishService {
    /// 发布协议到指定线程
    ///
    /// 此方法作为协调者，调用各个专门的函数完成协议发布流程
    pub async fn publish(
        http: &Http,
        data: &Data,
        thread: &GuildChannel,
        license: &entities::user_licenses::Model,
        backup_allowed: bool,
        author: User,
    ) -> Result<(), BotError> {
        // 1. 处理已有协议
        Self::handle_existing_license(http, data, thread).await?;

        // 2. 发布新协议消息
        let new_msg =
            Self::publish_new_message(http, thread, license, backup_allowed, &author).await?;

        // 3. 更新数据库记录
        let backup_changed =
            Self::update_database_records(data, thread, new_msg.id, author.id, backup_allowed)
                .await?;

        // 4. 发送备份通知（如果需要）
        Self::send_backup_notification_if_needed(
            http,
            data,
            thread,
            new_msg.id,
            &author,
            license,
            backup_allowed,
            backup_changed,
        )
        .await?;

        // 5. 增加使用计数
        Self::increment_usage_count(data, license.id, author.id).await?;

        Ok(())
    }

    /// 处理已有协议（标记为作废并取消置顶）
    async fn handle_existing_license(
        http: &Http,
        data: &Data,
        thread: &GuildChannel,
    ) -> Result<(), BotError> {
        let existing_post = data.db().published_posts().get_by_thread(thread.id).await?;

        if let Some(existing) = existing_post {
            // 编辑旧协议消息为作废
            if let Ok(mut old_msg) = http
                .get_message(thread.id, MessageId::new(existing.message_id as u64))
                .await
            {
                // 获取原有的 embed
                if let Some(original_embed) = old_msg.embeds.first() {
                    let fields: Vec<(String, String, bool)> = original_embed
                        .fields
                        .iter()
                        .map(|f| (f.name.clone(), f.value.clone(), f.inline))
                        .collect();

                    let footer_text = original_embed.footer.as_ref().map(|f| f.text.as_str());

                    let updated_embed = LicenseEmbedBuilder::create_obsolete_license_embed(
                        original_embed.title.as_deref().unwrap_or("授权协议"),
                        original_embed.description.as_deref().unwrap_or(""),
                        &fields,
                        footer_text,
                    );

                    let _ = old_msg
                        .edit(http, EditMessage::new().embed(updated_embed))
                        .await;
                }

                // Unpin旧消息
                let _ = old_msg.unpin(http).await;
            }
        }

        Ok(())
    }

    /// 发布新协议消息并置顶
    async fn publish_new_message(
        http: &Http,
        thread: &GuildChannel,
        license: &entities::user_licenses::Model,
        backup_allowed: bool,
        author: &User,
    ) -> Result<serenity::all::Message, BotError> {
        let display_name = thread
            .guild_id
            .member(http, author.id)
            .await
            .map(|m| m.display_name().to_string())
            .unwrap_or_else(|_| author.display_name().to_string());

        let license_embed =
            LicenseEmbedBuilder::create_license_embed(license, backup_allowed, &display_name);
        let new_msg = ChannelId::new(thread.id.get())
            .send_message(http, CreateMessage::new().embed(license_embed))
            .await?;

        // Pin新消息
        let _ = new_msg.pin(http).await;

        Ok(new_msg)
    }

    /// 更新数据库记录并检查备份权限变更
    async fn update_database_records(
        data: &Data,
        thread: &GuildChannel,
        message_id: MessageId,
        author_id: serenity::all::UserId,
        backup_allowed: bool,
    ) -> Result<bool, BotError> {
        // 检查备份权限是否变更
        let backup_changed = data
            .db()
            .published_posts()
            .has_backup_permission_changed(thread.id, backup_allowed)
            .await?;

        // 更新数据库
        data.db()
            .published_posts()
            .record_or_update(thread.id, message_id, author_id, backup_allowed)
            .await?;

        Ok(backup_changed)
    }

    /// 发送备份通知（如果权限发生变更）
    #[allow(clippy::too_many_arguments)]
    async fn send_backup_notification_if_needed(
        http: &Http,
        data: &Data,
        thread: &GuildChannel,
        message_id: MessageId,
        author: &User,
        license: &entities::user_licenses::Model,
        backup_allowed: bool,
        backup_changed: bool,
    ) -> Result<(), BotError> {
        if backup_changed {
            info!("备份权限发生变更，发送通知");

            // 获取帖子首楼消息作为内容预览
            let content_preview = Self::get_thread_first_message_content(http, thread)
                .await
                .unwrap_or_else(|_| "无法获取内容预览".to_string());

            let notification_payload = NotificationPayload::from_discord_context(
                thread,
                message_id,
                author.clone(),
                content_preview,
                license.license_name.clone(),
                backup_allowed,
            )
            .await;

            if let Err(e) = data
                .notification_service()
                .send_backup_notification(&notification_payload)
                .await
            {
                error!("发送备份通知失败: {}", e);
            }
        }

        Ok(())
    }

    /// 增加协议使用计数
    async fn increment_usage_count(
        data: &Data,
        license_id: i32,
        author_id: serenity::all::UserId,
    ) -> Result<(), BotError> {
        data.db()
            .license()
            .increment_usage(license_id, author_id)
            .await
    }

    /// 获取帖子首楼消息内容
    async fn get_thread_first_message_content(
        http: &Http,
        thread: &GuildChannel,
    ) -> Result<String, BotError> {
        // 尝试获取帖子的首楼消息
        // 通常帖子的首楼消息ID就是帖子ID本身
        let first_message = http
            .get_message(thread.id, MessageId::new(thread.id.get()))
            .await?;

        if !first_message.author.bot && !first_message.content.is_empty() {
            Ok(first_message.content)
        } else {
            Ok("该帖子暂无文本内容".to_string())
        }
    }
}
