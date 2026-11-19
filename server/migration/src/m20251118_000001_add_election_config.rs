use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE elections ADD COLUMN config JSON NOT NULL DEFAULT '{}'",
            )
            .await?;

        // Backfill from the grouped-specific columns. json() unwraps the JSON
        // text stored in those columns so it nests as real JSON, not a string.
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE elections SET config = json_object(
                    'election_type', election_type,
                    'groups', json(groups),
                    'candidate_groups', json(candidate_groups)
                ) WHERE election_type = 'stv-md-grouped'",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE elections SET config = json_object('election_type', election_type)
                WHERE election_type != 'stv-md-grouped'",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE elections DROP COLUMN config")
            .await?;
        Ok(())
    }
}
