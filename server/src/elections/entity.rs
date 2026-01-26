use chrono::{DateTime, Utc};
use sea_orm::{FromJsonQueryResult, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
pub struct Candidates(pub Vec<String>);

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "elections")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub uuid: String,
    pub admin_uuid: String,
    pub title: String,
    pub description: Option<String>,
    pub candidates: Candidates,
    pub num_seats: u32,
    pub ordered_seats: bool,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
