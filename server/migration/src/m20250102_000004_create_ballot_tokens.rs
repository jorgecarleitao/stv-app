use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE ballot_tokens (
                    election_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    created_at TIMESTAMP NOT NULL,
                    converted_at TIMESTAMP,
                    PRIMARY KEY (election_id, id),
                    FOREIGN KEY (election_id) REFERENCES elections(uuid)
                        ON DELETE CASCADE
                        ON UPDATE CASCADE
                )",
            )
            .await?;

        // Create index on election_id for listing all tokens in an election
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_ballot_tokens_election_id ON ballot_tokens(election_id)",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE ballot_tokens")
            .await?;

        Ok(())
    }
}
