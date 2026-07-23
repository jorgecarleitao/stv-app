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

pub(crate) async fn find_election_by_admin<C>(
    db: &C,
    election_id: &str,
    admin_uuid: &str,
) -> Result<entity::Model, Error>
where
    C: ConnectionTrait,
{
    Elections::find()
        .filter(entity::Column::Uuid.eq(election_id))
        .filter(entity::Column::AdminUuid.eq(admin_uuid))
        .one(db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)
}

fn election_type_to_string(t: ElectionType) -> String {
    match t {
        ElectionType::StvMd => "stv-md".to_string(),
        ElectionType::StvMdCoperland => "stv-md-coperland".to_string(),
        ElectionType::StvMdGrouped => "stv-md-grouped".to_string(),
    }
}

fn parse_election_type(s: &str) -> Result<ElectionType, Error> {
    match s {
        "stv-md" => Ok(ElectionType::StvMd),
        "stv-md-coperland" => Ok(ElectionType::StvMdCoperland),
        "stv-md-grouped" => Ok(ElectionType::StvMdGrouped),
        _ => Err(Error::Internal(format!("Invalid election_type '{}'", s))),
    }
}

fn build_election_response(model: entity::Model, is_locked: bool) -> Result<ElectionResponse, Error> {
    let election_type = parse_election_type(&model.election_type)?;
    let common = |uuid, admin_uuid, title, description, candidates, num_seats, start_time, end_time| {
        (uuid, admin_uuid, title, description, candidates, num_seats, start_time, end_time)
    };
    let (uuid, admin_uuid, title, description, candidates, num_seats, start_time, end_time) = common(
        model.uuid, model.admin_uuid, model.title, model.description,
        model.candidates.0, model.num_seats, model.start_time.into(), model.end_time.into(),
    );

    match election_type {
        ElectionType::StvMd => Ok(ElectionResponse::StvMd {
            uuid, admin_uuid, title, description, candidates, num_seats,
            start_time, end_time, is_locked,
        }),
        ElectionType::StvMdCoperland => Ok(ElectionResponse::StvMdCoperland {
            uuid, admin_uuid, title, description, candidates, num_seats,
            start_time, end_time, is_locked,
        }),
        ElectionType::StvMdGrouped => Ok(ElectionResponse::StvMdGrouped {
            uuid, admin_uuid, title, description, candidates, num_seats,
            start_time, end_time, is_locked,
            groups: model.groups.0,
            candidate_groups: model.candidate_groups.0,
        }),
    }
}

fn model_from_request(req: &CreateElectionRequest, uuid: String, admin_uuid: String) -> entity::ActiveModel {
    entity::ActiveModel {
        uuid: Set(uuid),
        admin_uuid: Set(admin_uuid),
        title: Set(req.title().to_string()),
        description: Set(req.description().clone()),
        candidates: Set(entity::Candidates(req.candidates().to_vec())),
        num_seats: Set(req.num_seats()),
        election_type: Set(election_type_to_string(req.election_type())),
        start_time: Set(*req.start_time()),
        end_time: Set(*req.end_time()),
        groups: Set(entity::Groups(req.groups().to_vec())),
        candidate_groups: Set(entity::CandidateGroups(req.candidate_groups().to_vec())),
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

    let election = model_from_request(&req, uuid, admin_uuid);

    let inserted = election
        .insert(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to create election: {}", e)))?;

    build_election_response(inserted, false)
        .map(Json)
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
    let election = find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    let is_locked = is_election_locked(&state.db, &election_id).await?;

    build_election_response(election, is_locked)
        .map(Json)
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

    let election = find_election_by_admin(&txn, &election_id, &admin_uuid).await?;

    // Check if title, description, or candidates are being modified
    let title_changed = election.title != req.title();
    let description_changed = election.description != req.description().clone();
    let candidates_changed = election.candidates.0 != req.candidates();

    let is_locked = is_election_locked(&txn, &election_id).await?;

    if (title_changed || description_changed || candidates_changed) && is_locked {
        return Err(Error::BadRequest(
            "Cannot modify election title, description, or candidates after ballots have been issued. Tokens have already been redeemed.".to_string()
        ));
    }

    let mut election_active: entity::ActiveModel = election.into();
    election_active.title = Set(req.title().to_string());
    election_active.description = Set(req.description().clone());
    election_active.candidates = Set(entity::Candidates(req.candidates().to_vec()));
    election_active.num_seats = Set(req.num_seats());
    election_active.election_type = Set(election_type_to_string(req.election_type()));
    election_active.start_time = Set(*req.start_time());
    election_active.end_time = Set(*req.end_time());
    election_active.groups = Set(entity::Groups(req.groups().to_vec()));
    election_active.candidate_groups = Set(entity::CandidateGroups(req.candidate_groups().to_vec()));

    let updated = election_active
        .update(&txn)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update election: {}", e)))?;

    txn.commit()
        .await
        .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

    build_election_response(updated, is_locked)
        .map(Json)
}
