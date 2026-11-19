use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, patch, post, put},
};
use chrono::Utc;
use sea_orm::{ColumnTrait, DbConn, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tracing;
use utoipa::{OpenApi, ToSchema};

pub mod ballot_tokens;
pub mod ballots;
pub mod counting;
pub mod election_results;
pub mod elections;
pub mod email;
pub mod email_config;
pub mod error;
pub mod export;
pub mod log;

/// ElectionConfig - what gets sent to the API (no sensitive info)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "election_type", rename_all = "kebab-case")]
pub enum ElectionConfig {
    StvMd {
        id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        candidates: Vec<String>,
        seats: u32,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
        number_of_ballots: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ballots: Option<Vec<String>>,
    },
    StvMdCoperland {
        id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        candidates: Vec<String>,
        seats: u32,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
        number_of_ballots: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ballots: Option<Vec<String>>,
    },
    StvMdGrouped {
        id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        candidates: Vec<String>,
        seats: u32,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
        number_of_ballots: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ballots: Option<Vec<String>>,
        groups: Vec<counting::GroupConfig>,
        candidate_groups: Vec<String>,
    },
}

impl ElectionConfig {
    pub fn id(&self) -> &str {
        match self {
            ElectionConfig::StvMd { id, .. } => id,
            ElectionConfig::StvMdCoperland { id, .. } => id,
            ElectionConfig::StvMdGrouped { id, .. } => id,
        }
    }
    pub fn candidates(&self) -> &[String] {
        match self {
            ElectionConfig::StvMd { candidates, .. } => candidates,
            ElectionConfig::StvMdCoperland { candidates, .. } => candidates,
            ElectionConfig::StvMdGrouped { candidates, .. } => candidates,
        }
    }
    pub fn seats(&self) -> u32 {
        match self {
            ElectionConfig::StvMd { seats, .. } => *seats,
            ElectionConfig::StvMdCoperland { seats, .. } => *seats,
            ElectionConfig::StvMdGrouped { seats, .. } => *seats,
        }
    }
    pub fn ballots(&self) -> &Option<Vec<String>> {
        match self {
            ElectionConfig::StvMd { ballots, .. } => ballots,
            ElectionConfig::StvMdCoperland { ballots, .. } => ballots,
            ElectionConfig::StvMdGrouped { ballots, .. } => ballots,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: DbConn,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ElectionState {
    /// Election configuration and metadata
    pub election: ElectionConfig,
    /// Number of ballot tokens issued (potential voters)
    pub potential_voters: usize,
    /// Number of ballots that have been cast
    pub casted: usize,
    /// Election results (only available after election ends)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<counting::ElectionResult>,
}

#[utoipa::path(
    get,
    path = "/api/health",
    responses(
        (status = 200, description = "Service is healthy")
    ),
    tag = "system"
)]
#[axum::debug_handler]
async fn health() -> Result<(), error::Error> {
    Ok(())
}

#[utoipa::path(
    get,
    path = "/api/elections/{election_uuid}",
    params(
        ("election_uuid" = String, Path, description = "Election UUID")
    ),
    responses(
        (status = 200, description = "Election state with results if available", body = ElectionState),
        (status = 404, description = "Election not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "elections"
)]
#[axum::debug_handler]
async fn get_election(
    Path(election_uuid): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ElectionState>, error::Error> {
    // Load election from database
    let election = elections::Elections::find_by_id(&election_uuid)
        .one(&state.db)
        .await
        .map_err(|e| error::Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(error::Error::NotFound)?;

    // Count ballot tokens for this election
    let all_tokens = ballot_tokens::BallotTokens::find()
        .filter(ballot_tokens::entity::Column::ElectionId.eq(&election_uuid))
        .all(&state.db)
        .await
        .map_err(|e| {
            error::Error::Internal(format!(
                "Failed to fetch tokens for {}: {}",
                election_uuid, e
            ))
        })?;

    let potential_voters = all_tokens.len();

    // Fetch all ballots for this election
    let all_ballots = ballots::Entity::find()
        .filter(ballots::Column::ElectionId.eq(&election_uuid))
        .all(&state.db)
        .await
        .map_err(|e| {
            error::Error::Internal(format!(
                "Failed to fetch ballots for {}: {}",
                election_uuid, e
            ))
        })?;

    let ballots: Vec<counting::Ballot> = all_ballots
        .iter()
        .filter_map(|b| b.data.clone().map(|data| data.into_ballot()))
        .collect();

    let casted = ballots.len();

    // Only publish results after the election has ended
    let now = Utc::now();
    let has_ended = now >= election.end_time;

    let results = if has_ended && casted > 0 {
        Some(
            get_or_compute_result(
                &state.db,
                &election_uuid,
                election.candidates.0.clone(),
                election.num_seats,
                ballots,
                &election.config,
            )
            .await?,
        )
    } else {
        None
    };

    // Convert database election to ElectionConfig format
    let ballots_opt = if has_ended {
        Some(all_ballots.into_iter().map(|b| b.id).collect())
    } else {
        None
    };
    let election_config = match &election.config {
        counting::SystemConfig::StvMd {} => ElectionConfig::StvMd {
            id: election.uuid, title: election.title, description: election.description,
            candidates: election.candidates.0.clone(), seats: election.num_seats,
            start_time: election.start_time, end_time: election.end_time,
            number_of_ballots: potential_voters, ballots: ballots_opt,
        },
        counting::SystemConfig::StvMdCoperland {} => ElectionConfig::StvMdCoperland {
            id: election.uuid, title: election.title, description: election.description,
            candidates: election.candidates.0.clone(), seats: election.num_seats,
            start_time: election.start_time, end_time: election.end_time,
            number_of_ballots: potential_voters, ballots: ballots_opt,
        },
        counting::SystemConfig::StvMdGrouped(cfg) => ElectionConfig::StvMdGrouped {
            id: election.uuid, title: election.title, description: election.description,
            candidates: election.candidates.0.clone(), seats: election.num_seats,
            start_time: election.start_time, end_time: election.end_time,
            number_of_ballots: potential_voters, ballots: ballots_opt,
            groups: cfg.groups().to_vec(), candidate_groups: cfg.candidate_groups().to_vec(),
        },
    };

    Ok(Json(ElectionState {
        election: election_config,
        potential_voters,
        casted,
        results,
    }))
}

#[utoipa::path(
    post,
    path = "/api/simulate",
    request_body = counting::Election,
    responses(
        (status = 200, description = "Election simulation results", body = counting::ElectionResult),
        (status = 400, description = "Invalid election data"),
        (status = 500, description = "Simulation failed")
    ),
    tag = "simulation"
)]
#[axum::debug_handler]
async fn simulate(
    payload: Result<Json<counting::Election>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<counting::ElectionResult>, error::Error> {
    // Deserialization goes through try_new, so invalid election data is
    // rejected here at parse time with a 400.
    let Json(election) =
        payload.map_err(|e| error::Error::BadRequest(format!("Invalid election data: {}", e)))?;
    counting::count(election.clone())
        .map_err(error::Error::BadRequest)
        .map(Json)
}

#[derive(Deserialize)]
struct ExportParams {
    #[serde(default)]
    include_emails: bool,
}

#[axum::debug_handler]
async fn export_election(
    Path(election_uuid): Path<String>,
    State(state): State<AppState>,
    Query(params): Query<ExportParams>,
) -> Result<axum::response::Response, error::Error> {
    let bytes = export::build_export_zip(&state.db, &election_uuid, params.include_emails).await?;
    let filename = format!("election-export-{}.zip", election_uuid);
    let disposition = format!("attachment; filename=\"{}\"", filename);
    let response = axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "application/zip")
        .header(axum::http::header::CONTENT_DISPOSITION, &disposition)
        .body(axum::body::Body::from(bytes))
        .map_err(|e| error::Error::Internal(format!("Failed to build response: {}", e)))?;
    Ok(response)
}

pub(crate) async fn get_or_compute_result(
    db: &DbConn,
    election_uuid: &str,
    candidates: Vec<String>,
    num_seats: u32,
    ballots: Vec<counting::Ballot>,
    config: &counting::SystemConfig,
) -> Result<counting::ElectionResult, error::Error> {
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    let cached = election_results::Entity::find_by_id(election_uuid)
        .one(db)
        .await
        .map_err(|e| {
            error::Error::Internal(format!(
                "Failed to query election_results for {}: {}",
                election_uuid, e
            ))
        })?;

    if let Some(stored) = cached {
        return serde_json::from_value(stored.result).map_err(|e| {
            error::Error::Internal(format!(
                "Failed to deserialize stored result for {}: {}",
                election_uuid, e
            ))
        });
    }

    let election = counting::to_election_with_config(
        candidates,
        num_seats as usize,
        ballots,
        config,
    )
    .map_err(|e| error::Error::Internal(format!("Invalid election data for {}: {}", election_uuid, e)))?;
    let result = counting::count(election).map_err(|e| {
        error::Error::Internal(format!(
            "Counting failed for {}: {:?}",
            election_uuid, e
        ))
    })?;

    let result_value = serde_json::to_value(&result).map_err(|e| {
        error::Error::Internal(format!(
            "Failed to serialize result for {}: {}",
            election_uuid, e
        ))
    })?;

    let _ = election_results::ActiveModel {
        election_id: Set(election_uuid.to_owned()),
        result: Set(result_value),
        computed_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to persist results for {}: {}", election_uuid, e);
        e
    });

    Ok(result)
}

#[derive(utoipa::OpenApi)]
#[openapi(
        paths(
            health,
            get_election,
            simulate,
            elections::handlers::create_election,
            elections::handlers::get_election_by_admin,
            elections::handlers::update_election_by_admin,
            ballot_tokens::handlers::get_ballot_tokens,
            ballot_tokens::handlers::create_ballot_tokens,
            ballot_tokens::handlers::redeem_token,
            ballot_tokens::handlers::get_token_info,
            ballot_tokens::handlers::send_emails,
            ballot_tokens::handlers::send_single_token,
            ballot_tokens::handlers::patch_token,
            ballot_tokens::handlers::batch_mark_sent,
            email_config::handlers::upsert_email_config,
            email_config::handlers::get_email_config,
            email_config::handlers::delete_email_config,
            email_config::handlers::send_test_email,
            ballots::handlers::get_ballots_by_election,
            ballots::handlers::get_ballot,
            ballots::handlers::put_ballot,
        ),
        components(
            schemas(
                ElectionConfig,
                ElectionState,
                counting::ElectionType,
                counting::SystemConfig,
                counting::Ballot,
                counting::CombinedBallot,
                counting::RankedBallot,
                counting::CastBallotRequest,
                counting::Election,
                counting::ElectionResult,
                counting::Elected,
                counting::GroupConfig,
                counting::GroupResult,
                log::CountingLog,
                log::CountingLogHeader,
                log::CountingLogCandidate,
                log::CountingLogRound,
                log::CountingLogAction,
                log::CountingLogActionType,
                log::CountingLogCandidateCount,
                log::CountingLogCandidateStatus,
                log::CountingLogStats,
                elections::CreateElectionRequest,
                elections::ElectionResponse,
                email_config::handlers::EmailConfigResponse,
                email_config::handlers::UpsertEmailConfigRequest,
                email_config::handlers::TestEmailRequest,
                ballot_tokens::handlers::SendEmailsRequest,
                ballot_tokens::handlers::SendEmailResult,
                ballot_tokens::handlers::MarkSentBody,
                ballot_tokens::handlers::BatchMarkSentBody,
                ballot_tokens::handlers::MarkSentResult,
            )
        ),
        tags(
            (name = "system", description = "System health endpoints"),
            (name = "elections", description = "Election management endpoints"),
            (name = "ballot-tokens", description = "Ballot token management endpoints"),
            (name = "email-config", description = "Email SMTP configuration endpoints"),
            (name = "ballots", description = "Ballot management endpoints"),
            (name = "simulation", description = "Election simulation endpoints")
        ),
        info(
            title = "STV Election API",
            version = "0.1.0",
            description = "API for Single Transferable Vote (STV) election system",
        )
    )]
pub struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

pub fn create_app(db: DbConn) -> Result<Router<()>, String> {
    use axum::http::Method;
    use tower_http::cors::{Any, CorsLayer};

    let state = AppState { db };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT])
        .allow_headers(Any);

    // Election routes nested under /api/elections
    let elections_router = Router::new()
        .route(
            "/",
            post(elections::handlers::create_election),
        )
        .route("/{election_id}", get(get_election))
        .route(
            "/{election_id}/admin/{admin_uuid}",
            get(elections::handlers::get_election_by_admin)
                .put(elections::handlers::update_election_by_admin),
        )
        .route(
            "/{election_id}/ballots",
            get(ballots::handlers::get_ballots_by_election),
        )
        .route(
            "/{election_id}/ballot/{uuid}",
            get(ballots::handlers::get_ballot).put(ballots::handlers::put_ballot),
        )
        .route(
            "/{election_id}/admin/{admin_uuid}/tokens",
            get(ballot_tokens::handlers::get_ballot_tokens)
                .post(ballot_tokens::handlers::create_ballot_tokens),
        )
        .route(
            "/{election_id}/admin/{admin_uuid}/tokens/send",
            post(ballot_tokens::handlers::send_emails),
        )
        .route(
            "/{election_id}/admin/{admin_uuid}/tokens/{token_id}/send",
            post(ballot_tokens::handlers::send_single_token),
        )
        .route(
            "/{election_id}/admin/{admin_uuid}/tokens/mark-sent",
            post(ballot_tokens::handlers::batch_mark_sent),
        )
        .route(
            "/{election_id}/admin/{admin_uuid}/tokens/{token_id}",
            patch(ballot_tokens::handlers::patch_token),
        )
        .route(
            "/{election_id}/admin/{admin_uuid}/email-config",
            put(email_config::handlers::upsert_email_config)
                .get(email_config::handlers::get_email_config)
                .delete(email_config::handlers::delete_email_config),
        )
        .route(
            "/{election_id}/admin/{admin_uuid}/email-config/test",
            post(email_config::handlers::send_test_email),
        )
        .route(
            "/{election_id}/tokens/{token_id}",
            get(ballot_tokens::handlers::get_token_info),
        )
        .route(
            "/{election_id}/tokens/{token_id}/redeem",
            post(ballot_tokens::handlers::redeem_token),
        )
        .route(
            "/{election_id}/export",
            get(export_election),
        );

    Ok(Router::new()
        .route("/api/health", get(health))
        .nest("/api/elections", elections_router)
        .route("/api/simulate", post(simulate))
        .route(
            "/api/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        .route(
            "/swagger-ui",
            get(|| async { axum::response::Html(include_str!("swagger-ui.html")) }),
        )
        .with_state(state)
        .layer(cors))
}
