use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, patch, post, put},
};
use chrono::Utc;
use sea_orm::{ColumnTrait, DbConn, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tracing;
use utoipa::ToSchema;

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
    pub fn groups(&self) -> &[counting::GroupConfig] {
        match self {
            ElectionConfig::StvMdGrouped { groups, .. } => groups,
            _ => &[],
        }
    }
    pub fn candidate_groups(&self) -> &[String] {
        match self {
            ElectionConfig::StvMdGrouped { candidate_groups, .. } => candidate_groups,
            _ => &[],
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

pub fn parse_election_type(s: &str) -> counting::ElectionType {
    match s {
        "stv-md-coperland" => counting::ElectionType::StvMdCoperland,
        "stv-md-grouped" => counting::ElectionType::StvMdGrouped,
        _ => counting::ElectionType::StvMd,
    }
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
        .filter_map(|b| {
            b.ranks.as_ref().map(|ranks| counting::Ballot {
                ranks: ranks.0.clone(),
            })
        })
        .collect();

    let casted = ballots.len();

    // Only publish results after the election has ended
    let now = Utc::now();
    let has_ended = now >= election.end_time;
    let election_type = parse_election_type(&election.election_type);

    let results = if has_ended && casted > 0 {
        Some(
            get_or_compute_result(
                &state.db,
                &election_uuid,
                election.candidates.0.clone(),
                election.num_seats,
                election_type,
                ballots,
                election.groups.0.clone(),
                election.candidate_groups.0.clone(),
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
    let election_config = match election_type {
        counting::ElectionType::StvMd => ElectionConfig::StvMd {
            id: election.uuid, title: election.title, description: election.description,
            candidates: election.candidates.0.clone(), seats: election.num_seats,
            start_time: election.start_time, end_time: election.end_time,
            number_of_ballots: potential_voters, ballots: ballots_opt,
        },
        counting::ElectionType::StvMdCoperland => ElectionConfig::StvMdCoperland {
            id: election.uuid, title: election.title, description: election.description,
            candidates: election.candidates.0.clone(), seats: election.num_seats,
            start_time: election.start_time, end_time: election.end_time,
            number_of_ballots: potential_voters, ballots: ballots_opt,
        },
        counting::ElectionType::StvMdGrouped => ElectionConfig::StvMdGrouped {
            id: election.uuid, title: election.title, description: election.description,
            candidates: election.candidates.0.clone(), seats: election.num_seats,
            start_time: election.start_time, end_time: election.end_time,
            number_of_ballots: potential_voters, ballots: ballots_opt,
            groups: election.groups.0.clone(), candidate_groups: election.candidate_groups.0.clone(),
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
    Json(election): Json<counting::Election>,
) -> Result<Json<counting::ElectionResult>, error::Error> {
    if election.candidates().is_empty() {
        return Err(error::Error::BadRequest(
            "Candidates list cannot be empty".to_string(),
        ));
    }
    if election.seats() == 0 || election.seats() > election.candidates().len() {
        return Err(error::Error::BadRequest(
            "Invalid number of seats".to_string(),
        ));
    }
    for ballot in election.ballots() {
        for &rank in &ballot.ranks {
            if let Some(idx) = rank {
                if idx >= election.candidates().len() {
                    return Err(error::Error::BadRequest(
                        "Invalid candidate index".to_string(),
                    ));
                }
            }
        }
    }

    counting::stv_droop(election.clone())
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
    election_type: counting::ElectionType,
    ballots: Vec<counting::Ballot>,
    groups: Vec<counting::GroupConfig>,
    candidate_groups: Vec<String>,
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

    let election = counting::to_election_with_groups(
        candidates,
        num_seats as usize,
        election_type,
        ballots,
        groups,
        candidate_groups,
    );
    let result = counting::stv_droop(election).map_err(|e| {
        error::Error::Internal(format!(
            "STV counting failed for {}: {:?}",
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

pub fn create_app(db: DbConn) -> Result<Router<()>, String> {
    use axum::http::Method;
    use tower_http::cors::{Any, CorsLayer};
    use utoipa::OpenApi;

    #[derive(OpenApi)]
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
            ballots::handlers::get_ballots_by_election,
            ballots::handlers::get_ballot,
            ballots::handlers::put_ballot,
        ),
        components(
            schemas(
                ElectionConfig,
                ElectionState,
                counting::ElectionType,
                counting::Ballot,
                counting::CombinedBallot,
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
    struct ApiDoc;

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
