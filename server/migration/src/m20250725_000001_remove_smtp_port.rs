use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE email_configs_temp (
                    election_id TEXT NOT NULL PRIMARY KEY
                        REFERENCES elections(uuid) ON DELETE CASCADE ON UPDATE CASCADE,
                    smtp_host TEXT NOT NULL,
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
            .execute_unprepared(
                "INSERT INTO email_configs_temp
                 SELECT election_id, smtp_host, smtp_username, smtp_password,
                        from_name, from_email, updated_at
                 FROM email_configs",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP TABLE email_configs")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE email_configs_temp RENAME TO email_configs")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE email_configs_temp (
                    election_id TEXT NOT NULL PRIMARY KEY
                        REFERENCES elections(uuid) ON DELETE CASCADE ON UPDATE CASCADE,
                    smtp_host TEXT NOT NULL,
                    smtp_port INTEGER NOT NULL DEFAULT 587,
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
            .execute_unprepared(
                "INSERT INTO email_configs_temp
                 SELECT election_id, smtp_host, 587, smtp_username, smtp_password,
                        from_name, from_email, updated_at
                 FROM email_configs",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP TABLE email_configs")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE email_configs_temp RENAME TO email_configs")
            .await?;

        Ok(())
    }
}
