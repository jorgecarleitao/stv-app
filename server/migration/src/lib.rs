pub use sea_orm_migration::prelude::*;

mod m20250101_000001_create_ballots;
mod m20250102_000001_create_elections;
mod m20250102_000003_recreate_ballots;
mod m20250102_000004_create_ballot_tokens;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250101_000001_create_ballots::Migration),
            Box::new(m20250102_000001_create_elections::Migration),
            Box::new(m20250102_000003_recreate_ballots::Migration),
            Box::new(m20250102_000004_create_ballot_tokens::Migration),
        ]
    }
}
