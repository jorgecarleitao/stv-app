use sea_orm::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "ballot_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub election_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub created_at: chrono::NaiveDateTime,
    pub converted_at: Option<chrono::NaiveDateTime>,
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
