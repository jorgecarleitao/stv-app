use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use super::{Ballots, Ranks, entity};
use crate::{AppState, counting, elections, error::Error};

#[axum::debug_handler]
pub async fn get_ballots_by_election(
    State(state): State<AppState>,
    Path(election_id): Path<String>,
) -> Result<Json<Vec<entity::Model>>, Error> {
    let ballots = Ballots::find()
        .filter(entity::Column::ElectionId.eq(election_id.clone()))
        .all(&state.db)
        .await
        .map_err(|e| {
            Error::Internal(format!(
                "Failed to fetch ballots for election {}: {}",
                election_id, e
            ))
        })?;

    Ok(Json(ballots))
}

#[axum::debug_handler]
pub async fn get_ballot(
    Path((election_id, uuid)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<entity::Model>, Error> {
    let ballot = Ballots::find()
        .filter(entity::Column::ElectionId.eq(&election_id))
        .filter(entity::Column::Id.eq(&uuid))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch ballot {}: {}", uuid, e)))?
        .ok_or(Error::NotFound)?;

    Ok(Json(ballot))
}

#[axum::debug_handler]
pub async fn put_ballot(
    Path((election_id, uuid)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(ballot): Json<counting::Ballot>,
) -> Result<Json<serde_json::Value>, Error> {
    use sea_orm::{DbErr, TransactionTrait};

    state
        .db
        .transaction::<_, (), DbErr>(|txn| {
            Box::pin(async move {
                // Load election and validate it exists
                let election = elections::Elections::find_by_id(&election_id)
                    .one(txn)
                    .await?
                    .ok_or(DbErr::RecordNotFound("Election not found".to_string()))?;

                // Verify the ballot belongs to this election
                let ballot_record = Ballots::find()
                    .filter(entity::Column::ElectionId.eq(&election_id))
                    .filter(entity::Column::Id.eq(&uuid))
                    .one(txn)
                    .await?
                    .ok_or(DbErr::RecordNotFound("Ballot not found".to_string()))?;

                // Validate ranks
                if ballot
                    .ranks
                    .iter()
                    .filter_map(|&c| c)
                    .any(|c| c >= election.candidates.0.len())
                {
                    return Err(DbErr::Custom("Invalid candidate index".to_string()));
                }

                // Enforce voting period
                let now = Utc::now();
                if now < election.start_time {
                    return Err(DbErr::Custom("Voting has not started".to_string()));
                }
                if now >= election.end_time {
                    return Err(DbErr::Custom("Voting has ended".to_string()));
                }

                // Update ballot (ballot must already exist, created via token conversion)
                let mut active: entity::ActiveModel = ballot_record.into();
                active.ranks = Set(Some(Ranks(ballot.ranks)));
                active.update(txn).await?;

                Ok(())
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(err)
            | sea_orm::TransactionError::Transaction(err) => match err {
                DbErr::RecordNotFound(_) => Error::NotFound,
                DbErr::Custom(msg) => Error::BadRequest(msg),
                _ => Error::Internal(format!("Failed to update ballot: {}", err)),
            },
        })?;

    Ok(Json(serde_json::json!("null")))
}
