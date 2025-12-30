use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create new ballots table with foreign key to elections
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE ballots (
                    id TEXT NOT NULL,
                    election_id TEXT NOT NULL,
                    ranks JSON,
                    PRIMARY KEY (election_id, id),
                    FOREIGN KEY (election_id) REFERENCES elections(uuid)
                        ON DELETE CASCADE
                        ON UPDATE CASCADE
                )",
            )
            .await?;

        // Create index on election_id for listing all ballots in an election
        manager
            .get_connection()
            .execute_unprepared("CREATE INDEX idx_ballots_election_id ON ballots(election_id)")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE ballots")
            .await?;

        Ok(())
    }
}
