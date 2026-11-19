use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE ballots ADD COLUMN data JSON")
            .await?;

        // Backfill from the ranks column as a tagged CastBallotRequest payload:
        // {"election_type": <election's type>, "ranks": <ranks>}.
        // Rows with ranks NULL (abstained) keep data NULL.
        // json() unwraps the JSON text stored in ranks so it nests as real JSON.
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE ballots SET data = json_object(
                    'election_type', (SELECT e.election_type FROM elections e WHERE e.uuid = ballots.election_id),
                    'ranks', json(ranks)
                ) WHERE ranks IS NOT NULL",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE ballots DROP COLUMN data")
            .await?;
        Ok(())
    }
}
