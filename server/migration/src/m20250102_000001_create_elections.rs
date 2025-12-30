use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE elections (
                    uuid TEXT PRIMARY KEY NOT NULL,
                    admin_uuid TEXT NOT NULL,
                    title TEXT NOT NULL,
                    description TEXT,
                    candidates JSON,
                    num_seats INTEGER NOT NULL,
                    start_time TIMESTAMP NOT NULL,
                    end_time TIMESTAMP NOT NULL
                )",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared("CREATE INDEX idx_elections_admin_uuid ON elections(admin_uuid)")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE elections")
            .await?;

        Ok(())
    }
}
