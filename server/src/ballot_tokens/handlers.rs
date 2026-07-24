use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use sea_orm::sea_query::Expr;
use serde::Deserialize;
use uuid::Uuid;

use super::{BallotTokens, entity};
use crate::{
    AppState,
    email::{self, SmtpConfig},
    email_config,
    error::Error,
};

// --- Create tokens ---

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CreateTokenResult {
    pub id: String,
    pub email: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTokensBody {
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub recipients: Option<Vec<String>>,
}

#[utoipa::path(
    post,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/tokens",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    request_body = CreateTokensBody,
    responses(
        (status = 200, description = "List of generated tokens", body = Vec < CreateTokenResult >),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
pub async fn create_ballot_tokens(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
    Json(body): Json<CreateTokensBody>,
) -> Result<Json<Vec<CreateTokenResult>>, Error> {
    use sea_orm::{DbErr, TransactionTrait};

    let (count, recipients) = match (body.count, body.recipients) {
        (Some(n), None) => (n, Vec::new()),
        (None, Some(emails)) => (emails.len() as u64, emails),
        (Some(_), Some(_)) => {
            return Err(Error::BadRequest(
                "Provide either 'count' or 'recipients', not both".to_string(),
            ))
        }
        (None, None) => {
            return Err(Error::BadRequest(
                "Provide either 'count' or 'recipients'".to_string(),
            ))
        }
    };

    {
        let mut seen = std::collections::HashSet::new();
        for email in &recipients {
            if !seen.insert(email.clone()) {
                return Err(Error::BadRequest(format!(
                    "Duplicate recipient email: {}",
                    email
                )));
            }
        }
    }

    let election = crate::elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    let tokens = state
        .db
        .transaction::<_, Vec<CreateTokenResult>, DbErr>(|txn| {
            let recipients = recipients.clone();
            Box::pin(async move {
                let mut tokens = Vec::new();
                let now = Utc::now();

                for (i, _) in (0..count).enumerate() {
                    let token_id = Uuid::new_v4().to_string();
                    let email = recipients.get(i).cloned();

                    let ballot_token = entity::ActiveModel {
                        election_id: Set(election.uuid.clone()),
                        id: Set(token_id.clone()),
                        created_at: Set(now),
                        converted_at: Set(None),
                        email: Set(email.clone()),
                        sent_at: Set(None),
                    };

                    ballot_token.insert(txn).await?;

                    tokens.push(CreateTokenResult {
                        id: token_id,
                        email,
                    });
                }

                Ok(tokens)
            })
        })
        .await
        .map_err(|e| Error::Internal(format!("Failed to create ballot tokens: {}", e)))?;

    Ok(Json(tokens))
}

// --- Send emails ---

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SendEmailsRequest {
    pub base_url: String,
    #[serde(default)]
    pub token_ids: Vec<String>,
}

#[derive(Clone, serde::Serialize, utoipa::ToSchema)]
pub struct SendEmailResult {
    pub token_id: String,
    pub error: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/tokens/send",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    request_body = SendEmailsRequest,
    responses(
        (status = 200, description = "Email send results", body = Vec < SendEmailResult >),
        (status = 400, description = "Email not configured or token already sent"),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
pub async fn send_emails(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
    Json(body): Json<SendEmailsRequest>,
) -> Result<Json<Vec<SendEmailResult>>, Error> {
    let election = crate::elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    let config_model = email_config::find_by_election(&state.db, &election_id).await?;

    let smtp_config = SmtpConfig {
        smtp_host: config_model.smtp_host,
        smtp_username: config_model.smtp_username,
        smtp_password: config_model.smtp_password,
        from_name: config_model.from_name,
        from_email: config_model.from_email,
    };

    let tokens = if body.token_ids.is_empty() {
        BallotTokens::find()
            .filter(entity::Column::ElectionId.eq(&election_id))
            .filter(entity::Column::Email.is_not_null())
            .filter(entity::Column::SentAt.is_null())
            .all(&state.db)
            .await
            .map_err(|e| Error::Internal(format!("Failed to query tokens: {}", e)))?
    } else {
        let tokens = BallotTokens::find()
            .filter(entity::Column::ElectionId.eq(&election_id))
            .filter(entity::Column::Id.is_in(body.token_ids.clone()))
            .filter(entity::Column::Email.is_not_null())
            .filter(entity::Column::SentAt.is_null())
            .all(&state.db)
            .await
            .map_err(|e| Error::Internal(format!("Failed to query tokens: {}", e)))?;

        if tokens.len() != body.token_ids.len() {
            return Err(Error::NotFound);
        }
        tokens
    };

    if tokens.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let election_title = &election.title;
    let results: Vec<SendEmailResult> = tokens
        .iter()
        .map(|token| {
            let to_email = token.email.as_deref().unwrap_or("");
            let result = email::send_token_email(
                &smtp_config,
                to_email,
                election_title,
                &election_id,
                &token.id,
                &body.base_url,
            );

            SendEmailResult {
                token_id: token.id.clone(),
                error: result.error,
            }
        })
        .collect();

    let successful_ids: Vec<String> = results
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.token_id.clone())
        .collect();

    if !successful_ids.is_empty() {
        BallotTokens::update_many()
            .col_expr(entity::Column::SentAt, Expr::value(Some(Utc::now())))
            .filter(entity::Column::Id.is_in(successful_ids))
            .exec(&state.db)
            .await
            .map_err(|e| Error::Internal(format!("Failed to update sent_at: {}", e)))?;
    }

    Ok(Json(results))
}

#[utoipa::path(
    post,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/tokens/{token_id}/send",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization"),
        ("token_id" = String, Path, description = "Token ID to send email to")
    ),
    request_body = SendEmailsRequest,
    responses(
        (status = 200, description = "Email send result", body = SendEmailResult),
        (status = 400, description = "Email not configured or token already sent"),
        (status = 404, description = "Election or token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
pub async fn send_single_token(
    State(state): State<AppState>,
    Path((election_id, admin_uuid, token_id)): Path<(String, String, String)>,
    Json(body): Json<SendEmailsRequest>,
) -> Result<Json<SendEmailResult>, Error> {
    let request = SendEmailsRequest {
        base_url: body.base_url,
        token_ids: vec![token_id],
    };

    let Json(results) = send_emails(
        State(state),
        Path((election_id, admin_uuid)),
        Json(request),
    )
    .await?;

    let result = results
        .into_iter()
        .next()
        .unwrap_or(SendEmailResult {
            token_id: String::new(),
            error: Some("Token not found".to_string()),
        });

    Ok(Json(result))
}

// --- Mark sent (self-delivered mode) ---

#[derive(Deserialize, utoipa::ToSchema)]
pub struct MarkSentBody {
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchMarkSentBody {
    pub sent_at: chrono::DateTime<chrono::Utc>,
    pub token_ids: std::collections::HashSet<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MarkSentResult {
    pub token_id: String,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    patch,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/tokens/{token_id}",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization"),
        ("token_id" = String, Path, description = "Token ID to mark as sent")
    ),
    request_body = MarkSentBody,
    responses(
        (status = 200, description = "Token marked as sent", body = MarkSentResult),
        (status = 404, description = "Election or token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
pub async fn patch_token(
    State(state): State<AppState>,
    Path((election_id, admin_uuid, token_id)): Path<(String, String, String)>,
    Json(body): Json<MarkSentBody>,
) -> Result<Json<MarkSentResult>, Error> {
    let _election = crate::elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    let token = BallotTokens::find()
        .filter(entity::Column::ElectionId.eq(&election_id))
        .filter(entity::Column::Id.eq(&token_id))
        .one(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query token: {}", e)))?
        .ok_or(Error::NotFound)?;

    let mut active: entity::ActiveModel = token.into();
    active.sent_at = Set(Some(body.sent_at));
    active.update(&state.db).await
        .map_err(|e| Error::Internal(format!("Failed to update token: {}", e)))?;

    Ok(Json(MarkSentResult {
        token_id,
        sent_at: body.sent_at,
    }))
}

#[utoipa::path(
    post,
    path = "/api/elections/{election_id}/admin/{admin_uuid}/tokens/mark-sent",
    params(
        ("election_id" = String, Path, description = "Election UUID"),
        ("admin_uuid" = String, Path, description = "Admin UUID for authorization")
    ),
    request_body = BatchMarkSentBody,
    responses(
        (status = 200, description = "Tokens marked as sent", body = Vec < MarkSentResult >),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "ballot-tokens"
)]
pub async fn batch_mark_sent(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
    Json(body): Json<BatchMarkSentBody>,
) -> Result<Json<Vec<MarkSentResult>>, Error> {
    let _election = crate::elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    let tokens = BallotTokens::find()
        .filter(entity::Column::ElectionId.eq(&election_id))
        .filter(entity::Column::Id.is_in(body.token_ids.clone()))
        .all(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query tokens: {}", e)))?;

    if tokens.len() != body.token_ids.len() {
        return Err(Error::NotFound);
    }

    BallotTokens::update_many()
        .col_expr(entity::Column::SentAt, Expr::value(Some(body.sent_at)))
        .filter(entity::Column::Id.is_in(body.token_ids.clone()))
        .exec(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update tokens: {}", e)))?;

    let results: Vec<MarkSentResult> = body
        .token_ids
        .into_iter()
        .map(|token_id| MarkSentResult {
            token_id,
            sent_at: body.sent_at,
        })
        .collect();

    Ok(Json(results))
}

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
pub async fn get_ballot_tokens(
    State(state): State<AppState>,
    Path((election_id, admin_uuid)): Path<(String, String)>,
) -> Result<Json<Vec<entity::Model>>, Error> {
    let election = crate::elections::handlers::find_election_by_admin(&state.db, &election_id, &admin_uuid).await?;

    let tokens = BallotTokens::find()
        .filter(entity::Column::ElectionId.eq(election.uuid))
        .all(&state.db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch ballot tokens: {}", e)))?;

    Ok(Json(tokens))
}

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
pub async fn redeem_token(
    State(state): State<AppState>,
    Path((election_id, token_id)): Path<(String, String)>,
) -> Result<Json<String>, Error> {
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

                let now = chrono::Utc::now();
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

                let mut active: entity::ActiveModel = ballot_token.into();
                active.converted_at = Set(Some(Utc::now()));
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
