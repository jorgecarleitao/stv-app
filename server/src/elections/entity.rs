use sea_orm::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sea_orm::sea_query::{ArrayType, Nullable, ValueTypeErr};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidates(pub Vec<String>);

impl From<Candidates> for sea_orm::Value {
    fn from(candidates: Candidates) -> Self {
        sea_orm::Value::Json(Some(Box::new(
            serde_json::to_value(candidates.0).expect("Failed to serialize candidates")
        )))
    }
}

impl sea_orm::TryGetable for Candidates {
    fn try_get_by<I: sea_orm::ColIdx>(res: &sea_orm::QueryResult, idx: I) -> Result<Self, sea_orm::TryGetError> {
        let json_value: serde_json::Value = res.try_get_by(idx).map_err(sea_orm::TryGetError::DbErr)?;
        let vec: Vec<String> = serde_json::from_value(json_value)
            .map_err(|e| sea_orm::TryGetError::DbErr(sea_orm::DbErr::Type(e.to_string())))?;
        Ok(Candidates(vec))
    }
}

impl sea_orm::sea_query::ValueType for Candidates {
    fn try_from(v: sea_orm::Value) -> Result<Self, sea_orm::sea_query::ValueTypeErr> {
        match v {
            sea_orm::Value::Json(Some(json)) => {
                let vec: Vec<String> = serde_json::from_value(*json)
                    .map_err(|_| ValueTypeErr)?;
                Ok(Candidates(vec))
            }
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "Candidates".to_string()
    }

    fn array_type() -> ArrayType {
        ArrayType::Json
    }

    fn column_type() -> sea_orm::ColumnType {
        sea_orm::ColumnType::Json
    }
}

impl Nullable for Candidates {
    fn null() -> sea_orm::Value {
        sea_orm::Value::Json(None)
    }
}

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
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
