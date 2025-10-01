use chrono::Utc;
use entities::user_licenses::*;
use sea_orm::{QueryOrder, QuerySelect, Set, prelude::*, sea_query::Expr};
use serenity::all::*;

use super::types::UserLicense;
use crate::{database::BotDatabase, error::BotError};

pub struct LicenseService<'a>(&'a BotDatabase);

impl BotDatabase {
    /// Get a reference to the license service
    pub fn license(&self) -> LicenseService<'_> {
        LicenseService(self)
    }
}

impl LicenseService<'_> {
    /// Create a new user license
    pub async fn create(
        &self,
        user_id: UserId,
        license_name: String,
        allow_redistribution: bool,
        allow_modification: bool,
        restrictions_note: Option<String>,
        allow_backup: bool,
    ) -> Result<UserLicense, BotError> {
        // 检查用户协议数量是否超过上限
        let current_count = self.get_user_license_count(user_id).await?;
        if current_count >= 5 {
            return Err(BotError::GenericError {
                message: "您最多只能创建5个协议，请先删除一些协议。".to_string(),
                source: None,
            });
        }

        let license = ActiveModel {
            user_id: Set(user_id.get() as i64),
            license_name: Set(license_name),
            allow_redistribution: Set(allow_redistribution),
            allow_modification: Set(allow_modification),
            restrictions_note: Set(restrictions_note),
            allow_backup: Set(allow_backup),
            usage_count: Set(0),
            created_at: Set(Utc::now()),
            ..Default::default()
        };

        let result = license.insert(self.0.inner()).await?;
        Ok(result)
    }

    /// Get all licenses for a user
    pub async fn get_user_licenses(&self, user_id: UserId) -> Result<Vec<UserLicense>, BotError> {
        Ok(Entity::find()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .order_by_desc(Column::CreatedAt)
            .all(self.0.inner())
            .await?)
    }

    /// Get a specific license by ID and user ID
    pub async fn get_license(
        &self,
        license_id: i32,
        user_id: UserId,
    ) -> Result<Option<UserLicense>, BotError> {
        Ok(Entity::find()
            .filter(
                Column::Id
                    .eq(license_id)
                    .and(Column::UserId.eq(user_id.get() as i64)),
            )
            .one(self.0.inner())
            .await?)
    }

    /// Update a user license (atomic operation)
    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        license_id: i32,
        user_id: UserId,
        license_name: String,
        allow_redistribution: bool,
        allow_modification: bool,
        restrictions_note: Option<String>,
        allow_backup: bool,
    ) -> Result<Option<UserLicense>, BotError> {
        // 执行原子更新
        let update_result = Entity::update_many()
            .col_expr(Column::LicenseName, Expr::value(license_name))
            .col_expr(
                Column::AllowRedistribution,
                Expr::value(allow_redistribution),
            )
            .col_expr(Column::AllowModification, Expr::value(allow_modification))
            .col_expr(Column::RestrictionsNote, Expr::value(restrictions_note))
            .col_expr(Column::AllowBackup, Expr::value(allow_backup))
            .filter(
                Column::Id
                    .eq(license_id)
                    .and(Column::UserId.eq(user_id.get() as i64)),
            )
            .exec(self.0.inner())
            .await?;

        // 如果更新成功，获取更新后的记录
        if update_result.rows_affected > 0 {
            self.get_license(license_id, user_id).await
        } else {
            Ok(None)
        }
    }

    /// Delete a user license
    pub async fn delete(&self, license_id: i32, user_id: UserId) -> Result<bool, BotError> {
        let result = Entity::delete_many()
            .filter(
                Column::Id
                    .eq(license_id)
                    .and(Column::UserId.eq(user_id.get() as i64)),
            )
            .exec(self.0.inner())
            .await?;

        Ok(result.rows_affected > 0)
    }

    /// Get license count for a user
    pub async fn get_user_license_count(&self, user_id: UserId) -> Result<u64, BotError> {
        Ok(Entity::find()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .count(self.0.inner())
            .await?)
    }

    /// Increment usage count for a license (atomic operation)
    pub async fn increment_usage(&self, license_id: i32, user_id: UserId) -> Result<(), BotError> {
        Entity::update_many()
            .col_expr(Column::UsageCount, Expr::col(Column::UsageCount).add(1))
            .filter(
                Column::Id
                    .eq(license_id)
                    .and(Column::UserId.eq(user_id.get() as i64)),
            )
            .exec(self.0.inner())
            .await?;

        Ok(())
    }

    /// Get licenses sorted by usage count (most used first)
    pub async fn get_user_licenses_by_usage(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UserLicense>, BotError> {
        Ok(Entity::find()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .order_by_desc(Column::UsageCount)
            .order_by_desc(Column::CreatedAt)
            .all(self.0.inner())
            .await?)
    }

    /// Get total usage count for all licenses of a user
    pub async fn get_user_total_usage(&self, user_id: UserId) -> Result<i32, BotError> {
        use sea_orm::sea_query::Expr;

        let result = Entity::find()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .select_only()
            .column_as(Expr::col(Column::UsageCount).sum(), "total_usage")
            .into_tuple::<Option<i32>>()
            .one(self.0.inner())
            .await?;

        Ok(result.flatten().unwrap_or(0))
    }

    /// Check if a license name already exists for a user
    pub async fn license_name_exists(
        &self,
        user_id: UserId,
        license_name: &str,
        exclude_id: Option<i32>,
    ) -> Result<bool, BotError> {
        let mut query = Entity::find().filter(
            Column::UserId
                .eq(user_id.get() as i64)
                .and(Column::LicenseName.eq(license_name)),
        );

        if let Some(exclude_id) = exclude_id {
            query = query.filter(Column::Id.ne(exclude_id));
        }

        Ok(query.one(self.0.inner()).await?.is_some())
    }

    /// Clear all licenses for a user (dangerous operation)
    pub async fn clear_user_licenses(&self, user_id: UserId) -> Result<u64, BotError> {
        let result = Entity::delete_many()
            .filter(Column::UserId.eq(user_id.get() as i64))
            .exec(self.0.inner())
            .await?;

        Ok(result.rows_affected)
    }
}
