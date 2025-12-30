use axum::{
    extract::{Path, State},
    Json,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::{error::Error, AppState};
use super::{entity, CreateElectionRequest, ElectionResponse, Elections};

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
        start_time: Set(req.start_time.map(|t| t.into())),
        end_time: Set(req.end_time.map(|t| t.into())),
    };

    let inserted = election.insert(&state.db).await
        .map_err(|e| Error::Internal(format!("Failed to create election: {}", e)))?;

    Ok(Json(ElectionResponse {
        uuid: inserted.uuid,
        admin_uuid: inserted.admin_uuid,
        title: inserted.title,
        description: inserted.description,
        candidates: inserted.candidates.0,
        num_seats: inserted.num_seats,
        start_time: inserted.start_time.map(|t| t.into()),
        end_time: inserted.end_time.map(|t| t.into()),
    }))
}

#[axum::debug_handler]
pub async fn get_election_by_admin(
    State(state): State<AppState>,
    Path(admin_uuid): Path<String>,
) -> Result<Json<ElectionResponse>, Error> {
    let election = Elections::find()
        .filter(entity::Column::AdminUuid.eq(admin_uuid))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)?;

    Ok(Json(ElectionResponse {
        uuid: election.uuid,
        admin_uuid: election.admin_uuid,
        title: election.title,
        description: election.description,
        candidates: election.candidates.0,
        num_seats: election.num_seats,
        start_time: election.start_time.map(|t| t.into()),
        end_time: election.end_time.map(|t| t.into()),
    }))
}

#[axum::debug_handler]
pub async fn update_election_by_admin(
    State(state): State<AppState>,
    Path(admin_uuid): Path<String>,
    Json(req): Json<CreateElectionRequest>,
) -> Result<Json<ElectionResponse>, Error> {
    let election = Elections::find()
        .filter(entity::Column::AdminUuid.eq(admin_uuid))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)?;

    let mut election_active: entity::ActiveModel = election.into();
    election_active.title = Set(req.title);
    election_active.description = Set(req.description);
    election_active.candidates = Set(entity::Candidates(req.candidates));
    election_active.num_seats = Set(req.num_seats);
    election_active.start_time = Set(req.start_time.map(|t| t.into()));
    election_active.end_time = Set(req.end_time.map(|t| t.into()));

    let updated = election_active.update(&state.db).await
        .map_err(|e| Error::Internal(format!("Failed to update election: {}", e)))?;

    Ok(Json(ElectionResponse {
        uuid: updated.uuid,
        admin_uuid: updated.admin_uuid,
        title: updated.title,
        description: updated.description,
        candidates: updated.candidates.0,
        num_seats: updated.num_seats,
        start_time: updated.start_time.map(|t| t.into()),
        end_time: updated.end_time.map(|t| t.into()),
    }))
}
