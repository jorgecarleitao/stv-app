use axum::{
    Json,
    extract::{Path, State},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
    TransactionTrait,
};
use uuid::Uuid;

use super::{CreateElectionRequest, ElectionResponse, Elections, entity};
use crate::{AppState, counting::ElectionType, error::Error};

/// Check if an election is locked (has any redeemed tokens)
async fn is_election_locked<C>(db: &C, election_id: &str) -> Result<bool, Error>
where
    C: ConnectionTrait,
{
    use crate::ballot_tokens::entity::{Column as TokenColumn, Entity as BallotTokens};
    let redeemed_token_count = BallotTokens::find()
        .filter(TokenColumn::ElectionId.eq(election_id))
        .filter(TokenColumn::ConvertedAt.is_not_null())
        .count(db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to check tokens: {}", e)))?;
    Ok(redeemed_token_count > 0)
}

fn election_type_to_string(t: ElectionType) -> String {
    match t {
        ElectionType::StvMd => "stv-md".to_string(),
        ElectionType::StvMdCoperland => "stv-md-coperland".to_string(),
    }
}

fn parse_election_type(s: &str) -> Result<ElectionType, Error> {
    match s {
        "stv-md" => Ok(ElectionType::StvMd),
        "stv-md-coperland" => Ok(ElectionType::StvMdCoperland),
        _ => Err(Error::Internal(format!("Invalid election_type '{}'", s))),
    }
}

#[utoipa::path(
    post,
    path = "/api/elections",
    request_body = CreateElectionRequest,
    responses(
        (status = 200, description = "Election created successfully", body = ElectionResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "elections"
)]
#[axum::debug_handler]
pub async fn create_election(
    State(state): State<AppState>,
    Json(req): Json<CreateElectionRequest>,
) -> Result<Json<ElectionResponse>, Error> {
    let uuid = Uuid::new_v4().to_string();
    let admin_uuid = Uuid::new_v4().to_string();

    let election = entity::ActiveModel {
        uuid: Set(uuid),
        admin_uuid: Set(admin_uuid),
        title: Set(req.title),
        description: Set(req.description),
        candidates: Set(entity::Candidates(req.candidates)),
        num_seats: Set(req.num_seats),
        election_type: Set(election_type_to_string(req.election_type)),
        start_time: Set(req.start_time.into()),
        end_time: Set(req.end_time.into()),
    };

    let inserted = election
        .insert(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to create election: {}", e)))?;

    Ok(Json(ElectionResponse {
        uuid: inserted.uuid,
        admin_uuid: inserted.admin_uuid,
        title: inserted.title,
        description: inserted.description,
        candidates: inserted.candidates.0,
        num_seats: inserted.num_seats,
        election_type: parse_election_type(&inserted.election_type)?,
        start_time: inserted.start_time.into(),
        end_time: inserted.end_time.into(),
        is_locked: false, // New elections are never locked
    }))
}

#[utoipa::path(
    get,
    path = "/api/elections/{election_id}/admin/{admin_uuid}",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    responses(
        (status = 200, description = "Election details retrieved", body = ElectionResponse),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "elections"
)]
#[axum::debug_handler]
pub async fn get_election_by_admin(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
) -> Result<Json<ElectionResponse>, Error> {
    let election = Elections::find()
        .filter(entity::Column::Uuid.eq(&election_id))
        .filter(entity::Column::AdminUuid.eq(&admin_uuid))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)?;

    let is_locked = is_election_locked(&state.db, &election_id).await?;

    Ok(Json(ElectionResponse {
        uuid: election.uuid,
        admin_uuid: election.admin_uuid,
        title: election.title,
        description: election.description,
        candidates: election.candidates.0,
        num_seats: election.num_seats,
        election_type: parse_election_type(&election.election_type)?,
        start_time: election.start_time.into(),
        end_time: election.end_time.into(),
        is_locked,
    }))
}

#[utoipa::path(
    put,
    path = "/api/elections/{election_id}/admin/{admin_uuid}",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    request_body = CreateElectionRequest,
    responses(
        (status = 200, description = "Election updated successfully", body = ElectionResponse),
        (status = 400, description = "Bad request - election is locked or invalid data"),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "elections"
)]
#[axum::debug_handler]
pub async fn update_election_by_admin(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
    Json(req): Json<CreateElectionRequest>,
) -> Result<Json<ElectionResponse>, Error> {
    let txn = state
        .db
        .begin()
        .await
        .map_err(|e| Error::Internal(format!("Failed to start transaction: {}", e)))?;

    let election = Elections::find()
        .filter(entity::Column::Uuid.eq(&election_id))
        .filter(entity::Column::AdminUuid.eq(&admin_uuid))
        .one(&txn)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)?;

    // Check if title, description, or candidates are being modified
    let title_changed = election.title != req.title;
    let description_changed = election.description != req.description;
    let candidates_changed = election.candidates.0 != req.candidates;

    let is_locked = is_election_locked(&txn, &election_id).await?;

    if (title_changed || description_changed || candidates_changed) && is_locked {
        return Err(Error::BadRequest(
            "Cannot modify election title, description, or candidates after ballots have been issued. Tokens have already been redeemed.".to_string()
        ));
    }

    let mut election_active: entity::ActiveModel = election.into();
    election_active.title = Set(req.title);
    election_active.description = Set(req.description);
    election_active.candidates = Set(entity::Candidates(req.candidates));
    election_active.num_seats = Set(req.num_seats);
    election_active.election_type = Set(election_type_to_string(req.election_type));
    election_active.start_time = Set(req.start_time.into());
    election_active.end_time = Set(req.end_time.into());

    let updated = election_active
        .update(&txn)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update election: {}", e)))?;

    txn.commit()
        .await
        .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

    Ok(Json(ElectionResponse {
        uuid: updated.uuid,
        admin_uuid: updated.admin_uuid,
        title: updated.title,
        description: updated.description,
        candidates: updated.candidates.0,
        num_seats: updated.num_seats,
        election_type: parse_election_type(&updated.election_type)?,
        start_time: updated.start_time.into(),
        end_time: updated.end_time.into(),
        is_locked,
    }))
}
