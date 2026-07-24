use sea_orm::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "email_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub election_id: String,
    pub smtp_host: String,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_name: String,
    pub from_email: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::super::elections::entity::Entity",
        from = "Column::ElectionId",
        to = "super::super::elections::entity::Column::Uuid",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    Election,
}

impl Related<super::super::elections::entity::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Election.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
