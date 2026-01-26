use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add ordered_seats column with default value of 1 (true, backward compatible)
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE elections ADD COLUMN ordered_seats INTEGER NOT NULL DEFAULT 1",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE elections DROP COLUMN ordered_seats")
            .await?;

        Ok(())
    }
}
