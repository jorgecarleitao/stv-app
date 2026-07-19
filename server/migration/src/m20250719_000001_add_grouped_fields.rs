use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE elections ADD COLUMN groups JSON NOT NULL DEFAULT '[]'",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE elections ADD COLUMN candidate_groups JSON NOT NULL DEFAULT '[]'",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE elections DROP COLUMN groups")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE elections DROP COLUMN candidate_groups")
            .await?;
        Ok(())
    }
}
