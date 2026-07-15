use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add election_type column with default
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE elections ADD COLUMN election_type TEXT NOT NULL DEFAULT 'stv-md-coperland'",
            )
            .await?;

        // Backfill from ordered_seats: 1 (true) -> 'stv-md-coperland', 0 (false) -> 'stv-md'
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE elections SET election_type = 'stv-md-coperland' WHERE ordered_seats = 1",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE elections SET election_type = 'stv-md' WHERE ordered_seats = 0",
            )
            .await?;

        // Drop the old ordered_seats column
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE elections DROP COLUMN ordered_seats")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add ordered_seats column back
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE elections ADD COLUMN ordered_seats INTEGER NOT NULL DEFAULT 1",
            )
            .await?;

        // Backfill from election_type
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE elections SET ordered_seats = 1 WHERE election_type = 'stv-md-coperland'",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE elections SET ordered_seats = 0 WHERE election_type = 'stv-md'",
            )
            .await?;

        // Drop election_type
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE elections DROP COLUMN election_type")
            .await?;

        Ok(())
    }
}
