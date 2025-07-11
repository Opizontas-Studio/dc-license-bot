use serenity::all::{
    ChannelId, CreateMessage, EditMessage, 
    GuildChannel, Http, MessageId, UserId
};
use tracing::{error, info};

use crate::{
    commands::Data,
    error::BotError,
    services::notification_service::NotificationPayload,
    utils::LicenseEmbedBuilder,
};

pub struct LicensePublishService;

impl LicensePublishService {
    /// 发布协议到指定线程
    /// 
    /// 此方法包含完整的协议发布业务逻辑：
    /// - 检查并标记旧协议为作废
    /// - 发布新协议消息
    /// - 置顶新消息
    /// - 更新数据库记录
    /// - 发送备份权限变更通知
    /// - 增加协议使用计数
    pub async fn publish(
        http: &Http,
        data: &Data,
        thread: &GuildChannel,
        license: &entities::user_licenses::Model,
        backup_allowed: bool,
        author_id: UserId,
        author_name: &str,
        display_name: &str,
    ) -> Result<(), BotError> {
        // 1. 检查是否已有协议
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

        // 2. 发布新协议
        let license_embed = LicenseEmbedBuilder::create_license_embed(license, backup_allowed, display_name);
        let new_msg = ChannelId::new(thread.id.get())
            .send_message(http, CreateMessage::new().embed(license_embed))
            .await?;

        // 3. Pin新消息
        let _ = new_msg.pin(http).await;

        // 4. 检查备份权限是否变更
        let backup_changed = data.db().published_posts()
            .has_backup_permission_changed(thread.id, backup_allowed)
            .await?;

        // 5. 更新数据库
        data.db().published_posts()
            .record_or_update(thread.id, new_msg.id, author_id, backup_allowed)
            .await?;

        // 6. 如果备份权限发生变更，发送通知
        if backup_changed {
            info!("备份权限发生变更，发送通知");
            
            // 获取帖子首楼消息作为内容预览
            let content_preview = Self::get_thread_first_message_content(http, thread).await
                .unwrap_or_else(|_| "无法获取内容预览".to_string());
            
            let notification_payload = NotificationPayload::from_discord_context(
                thread.guild_id,
                thread.parent_id.unwrap(), // 父频道ID
                thread.id,                 // 帖子ID
                new_msg.id,
                author_id,
                author_name.to_string(),
                display_name.to_string(),
                thread.name.clone(),
                content_preview,
                license.license_name.clone(),
                backup_allowed,
            ).await;
            
            if let Err(e) = data.notification_service().send_backup_notification(&notification_payload).await {
                error!("发送备份通知失败: {}", e);
            }
        }

        // 7. 增加协议使用计数
        data.db().license().increment_usage(license.id, author_id).await?;

        Ok(())
    }


    /// 获取帖子首楼消息内容
    async fn get_thread_first_message_content(
        http: &Http,
        thread: &GuildChannel,
    ) -> Result<String, BotError> {
        // 尝试获取帖子的首楼消息
        // 通常帖子的首楼消息ID就是帖子ID本身
        let first_message = http.get_message(thread.id, MessageId::new(thread.id.get())).await?;
        
        if !first_message.author.bot && !first_message.content.is_empty() {
            Ok(first_message.content)
        } else {
            Ok("该帖子暂无文本内容".to_string())
        }
    }
}