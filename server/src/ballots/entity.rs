use sea_orm::prelude::*;

use serde::{Deserialize, Serialize};

use crate::counting::CastBallotRequest;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "ballots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub election_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    // None: abstained - the ballot was redeemed but never cast
    // Some: the ballot was cast; payload is tagged by the election's `election_type`
    pub data: Option<CastBallotRequest>,
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
