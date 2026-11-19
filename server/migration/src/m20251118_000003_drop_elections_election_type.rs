use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The election type is now stored solely as the tag of the `config`
        // JSON column (SystemConfig), which was backfilled with it.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE elections DROP COLUMN election_type")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE elections ADD COLUMN election_type TEXT NOT NULL DEFAULT 'stv-md'",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE elections SET election_type = json_extract(config, '$.election_type')",
            )
            .await?;
        Ok(())
    }
}
