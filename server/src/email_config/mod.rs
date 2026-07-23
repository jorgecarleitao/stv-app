pub mod entity;
pub mod handlers;

// Re-export the types used by ballot_tokens handlers
pub use entity::{ActiveModel, Column, Entity, Model};
pub use sea_orm::EntityTrait;

pub type EmailConfigs = Entity;

use sea_orm::{ColumnTrait, QueryFilter};

/// Find email config by election_id, returning an Error if not found.
pub(crate) async fn find_by_election<C>(db: &C, election_id: &str) -> Result<Model, crate::error::Error>
where
    C: sea_orm::ConnectionTrait,
{
    Entity::find()
        .filter(Column::ElectionId.eq(election_id))
        .one(db)
        .await
        .map_err(|e| crate::error::Error::Internal(format!("Failed to query email config: {}", e)))?
        .ok_or(crate::error::Error::BadRequest(
            "Email not configured for this election. Set up SMTP configuration first.".to_string(),
        ))
}
