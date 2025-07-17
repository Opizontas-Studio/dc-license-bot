use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create user_licenses table
        manager
            .create_table(
                Table::create()
                    .table(UserLicenses::Table)
                    .if_not_exists()
                    .col(pk_auto(UserLicenses::Id))
                    .col(big_unsigned(UserLicenses::UserId))
                    .col(string(UserLicenses::LicenseName))
                    .col(boolean(UserLicenses::AllowRedistribution))
                    .col(boolean(UserLicenses::AllowModification))
                    .col(string_null(UserLicenses::RestrictionsNote))
                    .col(boolean(UserLicenses::AllowBackup))
                    .col(integer(UserLicenses::UsageCount).default(0))
                    .col(timestamp(UserLicenses::CreatedAt).default(Expr::current_timestamp()))
                    .to_owned(),
            )
            .await?;

        // Create index for user_licenses.user_id
        manager
            .create_index(
                Index::create()
                    .name("idx_user_licenses_user_id")
                    .table(UserLicenses::Table)
                    .col(UserLicenses::UserId)
                    .to_owned(),
            )
            .await?;

        // Create user_settings table
        manager
            .create_table(
                Table::create()
                    .table(UserSettings::Table)
                    .if_not_exists()
                    .col(big_unsigned_uniq(UserSettings::UserId).primary_key())
                    .col(boolean(UserSettings::AutoPublishEnabled).default(false))
                    .col(boolean(UserSettings::SkipAutoPublishConfirmation).default(false))
                    .col(integer_null(UserSettings::DefaultUserLicenseId))
                    .col(string_null(UserSettings::DefaultSystemLicenseName))
                    .col(boolean_null(UserSettings::DefaultSystemLicenseBackup))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_settings_default_user_license")
                            .from(UserSettings::Table, UserSettings::DefaultUserLicenseId)
                            .to(UserLicenses::Table, UserLicenses::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // Create published_posts table
        manager
            .create_table(
                Table::create()
                    .table(PublishedPosts::Table)
                    .if_not_exists()
                    .col(big_unsigned(PublishedPosts::ThreadId).primary_key())
                    .col(big_unsigned(PublishedPosts::MessageId))
                    .col(big_unsigned(PublishedPosts::UserId))
                    .col(boolean(PublishedPosts::BackupAllowed))
                    .col(timestamp(PublishedPosts::UpdatedAt).default(Expr::current_timestamp()))
                    .to_owned(),
            )
            .await?;

        // Create index for published_posts.user_id
        manager
            .create_index(
                Index::create()
                    .name("idx_published_posts_user_id")
                    .table(PublishedPosts::Table)
                    .col(PublishedPosts::UserId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PublishedPosts::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(UserSettings::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(UserLicenses::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserLicenses {
    Table,
    Id,
    UserId,
    LicenseName,
    AllowRedistribution,
    AllowModification,
    RestrictionsNote,
    AllowBackup,
    UsageCount,
    CreatedAt,
}

#[derive(DeriveIden)]
enum UserSettings {
    Table,
    UserId,
    AutoPublishEnabled,
    SkipAutoPublishConfirmation,
    DefaultUserLicenseId,
    DefaultSystemLicenseName,
    DefaultSystemLicenseBackup,
}

#[derive(DeriveIden)]
enum PublishedPosts {
    Table,
    ThreadId,
    MessageId,
    UserId,
    BackupAllowed,
    UpdatedAt,
}
