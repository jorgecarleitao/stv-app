use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Store SMTP credentials in plaintext intentionally.
        //
        // The DB volume is encrypted at rest (AES-256 via Hetzner Block Storage),
        // so the password already has infrastructure-level protection.
        // Application-level encryption would require a separate decryption key
        // accessible to the server at runtime — moving the trust boundary without
        // meaningfully changing the threat model (both paths converge to "server
        // access implies credential access"). The email_config table has the same
        // access control as admin_uuid — only accessible via the election's
        // admin_uuid bearer token.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE email_configs (
                    election_id TEXT NOT NULL PRIMARY KEY
                        REFERENCES elections(uuid) ON DELETE CASCADE ON UPDATE CASCADE,
                    smtp_host TEXT NOT NULL,
                    smtp_port INTEGER NOT NULL,
                    smtp_username TEXT NOT NULL,
                    smtp_password TEXT NOT NULL,
                    from_name TEXT NOT NULL,
                    from_email TEXT NOT NULL,
                    updated_at TIMESTAMP NOT NULL
                )",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE ballot_tokens ADD COLUMN email TEXT")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE ballot_tokens ADD COLUMN sent_at TIMESTAMP")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE email_configs")
            .await?;

        // SQLite doesn't support DROP COLUMN, so we recreate without the columns.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE ballot_tokens_temp (
                    election_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    created_at TIMESTAMP NOT NULL,
                    converted_at TIMESTAMP,
                    PRIMARY KEY (election_id, id),
                    FOREIGN KEY (election_id) REFERENCES elections(uuid)
                        ON DELETE CASCADE ON UPDATE CASCADE
                )",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO ballot_tokens_temp (election_id, id, created_at, converted_at)
                 SELECT election_id, id, created_at, converted_at FROM ballot_tokens",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP TABLE ballot_tokens")
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE ballot_tokens_temp RENAME TO ballot_tokens",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_ballot_tokens_election_id ON ballot_tokens(election_id)",
            )
            .await?;

        Ok(())
    }
}
