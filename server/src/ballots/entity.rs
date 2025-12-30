use sea_orm::FromJsonQueryResult;
use sea_orm::prelude::*;

use serde::{Deserialize, Serialize};

/// Represents ballot ranks as Vec<Option<usize>>
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
pub struct Ranks(pub Vec<Option<usize>>);

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "ballots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub election_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    // None: abstentained - the ballot was redeemed but never casted
    // Some(vec![]): empty ballot - the voter casted but did not rank any candidates
    pub ranks: Option<Ranks>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::super::elections::entity::Entity",
        from = "Column::ElectionId",
        to = "super::super::elections::entity::Column::Uuid"
    )]
    Election,
}

impl Related<super::super::elections::entity::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Election.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
