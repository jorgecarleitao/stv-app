pub use sea_orm_migration::prelude::*;

mod m20250102_000001_create_elections;
mod m20250102_000002_create_ballots;
mod m20250102_000003_create_ballot_tokens;
mod m20250126_000001_add_ordered_seats;
mod m20250715_000001_add_election_type;
mod m20250716_000001_create_election_results;
mod m20250719_000001_add_grouped_fields;
mod m20250722_000001_add_email_config;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250102_000001_create_elections::Migration),
            Box::new(m20250102_000002_create_ballots::Migration),
            Box::new(m20250102_000003_create_ballot_tokens::Migration),
            Box::new(m20250126_000001_add_ordered_seats::Migration),
            Box::new(m20250715_000001_add_election_type::Migration),
            Box::new(m20250716_000001_create_election_results::Migration),
            Box::new(m20250719_000001_add_grouped_fields::Migration),
            Box::new(m20250722_000001_add_email_config::Migration),
        ]
    }
}
