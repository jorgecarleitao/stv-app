use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Ballots::Table)
                    .col(
                        ColumnDef::new(Ballots::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Ballots::ElectionId).string().not_null())
                    .col(ColumnDef::new(Ballots::BallotContent).json().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_ballots_election_id")
                    .table(Ballots::Table)
                    .col(Ballots::ElectionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Ballots::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Ballots {
    Table,
    Id,
    ElectionId,
    BallotContent,
}
