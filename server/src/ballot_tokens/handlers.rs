use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use super::{BallotTokens, entity};
use crate::{AppState, elections, error::Error};

/// Get all ballot tokens for an election by admin_uuid (admin only)
#[utoipa::path(
    get,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/tokens",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    responses(
        (status = 200, description = "List of ballot tokens"),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
#[axum::debug_handler]
pub async fn get_ballot_tokens(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
) -> Result<Json<Vec<entity::Model>>, Error> {
    // Get election by both election_id and admin_uuid to verify ownership
    let election = elections::Elections::find()
        .filter(elections::entity::Column::Uuid.eq(&election_id))
        .filter(elections::entity::Column::AdminUuid.eq(&admin_uuid))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)?;

    let tokens = BallotTokens::find()
        .filter(entity::Column::ElectionId.eq(election.uuid))
        .all(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch ballot tokens: {}", e)))?;

    Ok(Json(tokens))
}

/// Create ballot tokens for an election by admin_uuid (admin only)
#[utoipa::path(
    post,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/tokens",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    request_body = usize,
    responses(
        (status = 200, description = "List of generated token IDs", body = Vec<String>),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
#[axum::debug_handler]
pub async fn create_ballot_tokens(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
    Json(count): Json<usize>,
) -> Result<Json<Vec<String>>, Error> {
    use sea_orm::{DbErr, TransactionTrait};

    // Get election by both election_id and admin_uuid to verify ownership
    let election = elections::Elections::find()
        .filter(elections::entity::Column::Uuid.eq(&election_id))
        .filter(elections::entity::Column::AdminUuid.eq(&admin_uuid))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)?;

    let tokens = state
        .db
        .transaction::<_, Vec<String>, DbErr>(|txn| {
            Box::pin(async move {
                let mut tokens = Vec::new();
                let now = Utc::now().naive_utc();

                for _ in 0..count {
                    let token_id = Uuid::new_v4().to_string();

                    let ballot_token = entity::ActiveModel {
                        election_id: Set(election.uuid.clone()),
                        id: Set(token_id.clone()),
                        created_at: Set(now),
                        converted_at: Set(None),
                    };

                    ballot_token.insert(txn).await?;

                    tokens.push(token_id);
                }

                Ok(tokens)
            })
        })
        .await
        .map_err(|e| Error::Internal(format!("Failed to create ballot tokens: {}", e)))?;

    Ok(Json(tokens))
}

/// Redeem a token to create a ballot (voter action)
#[utoipa::path(
    post,
    path = "/api/elections/{election_id}/tokens/{token_id}/redeem",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("token_id" = String, Path, description = "Ballot token ID to redeem")
    ),
    responses(
        (status = 200, description = "Ballot ID created", body = String),
        (status = 400, description = "Token already redeemed or election closed"),
        (status = 404, description = "Token or election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
#[axum::debug_handler]
pub async fn redeem_token(
    State(state): State<AppState>,
    Path((election_id, token_id)): Path<(String, String)>,
) -> Result<Json<String>, Error> {
    use chrono::Utc;
    use sea_orm::{DbErr, TransactionTrait};

    let ballot_id = state
        .db
        .transaction::<_, String, DbErr>(|txn| {
            Box::pin(async move {
                // Fetch the election to check end_time inside the transaction
                let election = crate::elections::Elections::find()
                    .filter(crate::elections::entity::Column::Uuid.eq(&election_id))
                    .one(txn)
                    .await?
                    .ok_or(DbErr::RecordNotFound("Election not found".to_string()))?;

                let now = Utc::now();
                if now >= election.end_time {
                    return Err(DbErr::Custom(
                        "Election is closed. Tokens can no longer be redeemed.".to_string(),
                    ));
                }

                let ballot_token = BallotTokens::find()
                    .filter(entity::Column::ElectionId.eq(&election_id))
                    .filter(entity::Column::Id.eq(&token_id))
                    .one(txn)
                    .await?
                    .ok_or(DbErr::RecordNotFound("Token not found".to_string()))?;

                // Check if token is already redeemed
                if ballot_token.converted_at.is_some() {
                    return Err(DbErr::Custom("Token already redeemed".to_string()));
                }

                // Generate new ballot UUID
                let ballot_id = Uuid::new_v4().to_string();

                // NOTE: this is the moment the information token <> voter is erased - ballot does not reference token

                // Create the ballot
                let ballot = crate::ballots::entity::ActiveModel {
                    election_id: Set(ballot_token.election_id.clone()),
                    id: Set(ballot_id.clone()),
                    ranks: Set(None),
                };

                ballot.insert(txn).await?;

                // Update token with redeemed timestamp (converted_at column)
                let mut active: entity::ActiveModel = ballot_token.into();
                active.converted_at = Set(Some(Utc::now().naive_utc()));
                active.update(txn).await?;

                Ok(ballot_id)
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(err)
            | sea_orm::TransactionError::Transaction(err) => match err {
                DbErr::RecordNotFound(msg) if msg.contains("Election not found") => Error::NotFound,
                DbErr::RecordNotFound(_) => Error::NotFound,
                DbErr::Custom(msg) if msg == "Token already redeemed" => Error::BadRequest(msg),
                DbErr::Custom(msg)
                    if msg == "Election is closed. Tokens can no longer be redeemed." =>
                {
                    Error::BadRequest(msg)
                }
                _ => Error::Internal(format!("Failed to redeem token: {}", err)),
            },
        })?;

    Ok(Json(ballot_id))
}

/// Get token info (without redeeming it) - voter can check if already redeemed
#[utoipa::path(
    get,
    path = "/api/elections/{election_id}/tokens/{token_id}",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("token_id" = String, Path, description = "Ballot token ID")
    ),
    responses(
        (status = 200, description = "Token information"),
        (status = 404, description = "Token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
#[axum::debug_handler]
pub async fn get_token_info(
    State(state): State<AppState>,
    Path((election_id, token_id)): Path<(String, String)>,
) -> Result<Json<entity::Model>, Error> {
    let ballot_token = BallotTokens::find()
        .filter(entity::Column::ElectionId.eq(&election_id))
        .filter(entity::Column::Id.eq(&token_id))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch token: {}", e)))?
        .ok_or(Error::NotFound)?;

    Ok(Json(ballot_token))
}
