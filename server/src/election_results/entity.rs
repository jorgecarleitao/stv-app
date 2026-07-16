use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "election_results")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub election_id: String,
    pub result: Json,
    pub computed_at: DateTime<Utc>,
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
