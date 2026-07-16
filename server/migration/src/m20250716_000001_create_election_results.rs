use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE election_results (
                    election_id TEXT NOT NULL PRIMARY KEY
                        REFERENCES elections(uuid) ON DELETE CASCADE ON UPDATE CASCADE,
                    result JSON NOT NULL,
                    computed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                )",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS election_results")
            .await?;
        Ok(())
    }
}
