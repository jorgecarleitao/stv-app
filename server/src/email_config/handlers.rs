use axum::{Json, extract::{Path, State}};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use super::entity;
use crate::{AppState, elections, error::Error};

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct EmailConfigResponse {
    pub smtp_host: String,
    pub smtp_port: i32,
    pub smtp_username: String,
    pub from_name: String,
    pub from_email: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct UpsertEmailConfigRequest {
    pub smtp_host: String,
    pub smtp_port: i32,
    pub smtp_username: String,
    #[serde(default)]
    pub smtp_password: Option<String>,
    pub from_name: String,
    pub from_email: String,
}

#[utoipa::path(
    put,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/email-config",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    request_body = UpsertEmailConfigRequest,
    responses(
        (status = 200, description = "Email config created or updated", body = EmailConfigResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "email-config"
)]
pub async fn upsert_email_config(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
    Json(body): Json<UpsertEmailConfigRequest>,
) -> Result<Json<EmailConfigResponse>, Error> {
    let _election = elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    let now = Utc::now();

    let existing = super::Entity::find()
        .filter(entity::Column::ElectionId.eq(&election_id))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query email config: {}", e)))?;

    if let Some(existing) = existing {
        let mut active: entity::ActiveModel = existing.into();
        active.smtp_host = Set(body.smtp_host.clone());
        active.smtp_port = Set(body.smtp_port);
        active.smtp_username = Set(body.smtp_username.clone());
        if let Some(pwd) = body.smtp_password {
            active.smtp_password = Set(pwd);
        }
        active.from_name = Set(body.from_name.clone());
        active.from_email = Set(body.from_email.clone());
        active.updated_at = Set(now);
        active.update(&state.db).await.map_err(|e| {
            Error::Internal(format!("Failed to update email config: {}", e))
        })?;
    } else {
        let active = entity::ActiveModel {
            election_id: Set(election_id),
            smtp_host: Set(body.smtp_host.clone()),
            smtp_port: Set(body.smtp_port),
            smtp_username: Set(body.smtp_username.clone()),
            smtp_password: Set(body.smtp_password.unwrap_or_default()),
            from_name: Set(body.from_name.clone()),
            from_email: Set(body.from_email.clone()),
            updated_at: Set(now),
        };
        active.insert(&state.db).await.map_err(|e| {
            Error::Internal(format!("Failed to create email config: {}", e))
        })?;
    }

    Ok(Json(EmailConfigResponse {
        smtp_host: body.smtp_host,
        smtp_port: body.smtp_port,
        smtp_username: body.smtp_username,
        from_name: body.from_name,
        from_email: body.from_email,
    }))
}

#[utoipa::path(
    get,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/email-config",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    responses(
        (status = 200, description = "Email config (password omitted)", body = EmailConfigResponse),
        (status = 404, description = "Election not found or config not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "email-config"
)]
pub async fn get_email_config(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
) -> Result<Json<EmailConfigResponse>, Error> {
    let _election = elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    super::Entity::find()
        .filter(entity::Column::ElectionId.eq(&election_id))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query email config: {}", e)))?
        .ok_or(Error::NotFound)
        .map(|c| Json(EmailConfigResponse {
            smtp_host: c.smtp_host,
            smtp_port: c.smtp_port,
            smtp_username: c.smtp_username,
            from_name: c.from_name,
            from_email: c.from_email,
        }))
}

#[utoipa::path(
    delete,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/email-config",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    responses(
        (status = 200, description = "Email config deleted"),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "email-config"
)]
pub async fn delete_email_config(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
) -> Result<Json<()>, Error> {
    let _election = elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    super::Entity::delete_by_id(election_id)
        .exec(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to delete email config: {}", e)))?;

    Ok(Json(()))
}
