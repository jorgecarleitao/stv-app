use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
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
pub mod error;
pub mod log;

/// ElectionConfig - what gets sent to the API (no sensitive info)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ElectionConfig {
    /// Unique identifier for the election
    pub id: String,
    /// Title of the election
    pub title: String,
    /// Optional description providing additional details about the election
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// List of candidate names
    pub candidates: Vec<String>,
    /// Number of seats to be filled in the election
    pub seats: u32,
    /// Type of election algorithm
    pub election_type: counting::ElectionType,
    /// Election start time (ISO 8601 format)
    pub start_time: chrono::DateTime<Utc>,
    /// Election end time (ISO 8601 format)
    pub end_time: chrono::DateTime<Utc>,
    /// Total number of ballot tokens issued for this election
    pub number_of_ballots: usize,
    /// List of ballot IDs (only visible after election ends)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ballots: Option<Vec<String>>,
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

fn parse_election_type(s: &str) -> counting::ElectionType {
    match s {
        "stv-md-coperland" => counting::ElectionType::StvMdCoperland,
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
    path = "/api/elections",
    responses(
        (status = 200, description = "List of all elections", body = Vec<ElectionConfig>),
        (status = 500, description = "Internal server error")
    ),
    tag = "elections"
)]
#[axum::debug_handler]
async fn list_elections(
    State(state): State<AppState>,
) -> Result<Json<Vec<ElectionConfig>>, error::Error> {
    let elections = elections::Elections::find()
        .all(&state.db)
        .await
        .map_err(|e| error::Error::Internal(format!("Failed to load elections: {}", e)))?;

    let configs: Vec<ElectionConfig> = elections
        .into_iter()
        .map(|e| ElectionConfig {
            id: e.uuid,
            title: e.title,
            description: e.description,
            candidates: e.candidates.0,
            seats: e.num_seats,
            election_type: parse_election_type(&e.election_type),
            start_time: e.start_time,
            end_time: e.end_time,
            number_of_ballots: 0, // Would need to query tokens to get accurate count
            ballots: None,
        })
        .collect();

    Ok(Json(configs))
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
            )
            .await?,
        )
    } else {
        None
    };

    // Convert database election to ElectionConfig format
    let election_config = ElectionConfig {
        id: election.uuid,
        title: election.title,
        description: election.description,
        candidates: election.candidates.0.clone(),
        seats: election.num_seats,
        election_type,
        start_time: election.start_time,
        end_time: election.end_time,
        number_of_ballots: potential_voters,
        ballots: if has_ended {
            Some(all_ballots.into_iter().map(|b| b.id).collect())
        } else {
            None
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
    if election.candidates.is_empty() {
        return Err(error::Error::BadRequest(
            "Candidates list cannot be empty".to_string(),
        ));
    }
    if election.seats == 0 || election.seats > election.candidates.len() {
        return Err(error::Error::BadRequest(
            "Invalid number of seats".to_string(),
        ));
    }
    for ballot in &election.ballots {
        for &rank in &ballot.ranks {
            if let Some(idx) = rank {
                if idx >= election.candidates.len() {
                    return Err(error::Error::BadRequest(
                        "Invalid candidate index".to_string(),
                    ));
                }
            }
        }
    }

    counting::stv_droop(election.clone())
        .map_err(|e| error::Error::Internal(format!("Simulation failed: {:?}", e)))
        .map(Json)
}

async fn get_or_compute_result(
    db: &DbConn,
    election_uuid: &str,
    candidates: Vec<String>,
    num_seats: u32,
    election_type: counting::ElectionType,
    ballots: Vec<counting::Ballot>,
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

    let election = counting::to_election(candidates, num_seats as usize, election_type, ballots);
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
            list_elections,
            get_election,
            simulate,
            elections::handlers::create_election,
            elections::handlers::get_election_by_admin,
            elections::handlers::update_election_by_admin,
            ballot_tokens::handlers::get_ballot_tokens,
            ballot_tokens::handlers::create_ballot_tokens,
            ballot_tokens::handlers::redeem_token,
            ballot_tokens::handlers::get_token_info,
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
            )
        ),
        tags(
            (name = "system", description = "System health endpoints"),
            (name = "elections", description = "Election management endpoints"),
            (name = "ballot-tokens", description = "Ballot token management endpoints"),
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

    let static_dir_path =
        std::env::var("FRONTEND_STATIC_DIR").unwrap_or_else(|_| "app/static".to_string());
    let static_dir = tower_http::services::ServeDir::new(&static_dir_path).fallback(
        tower_http::services::ServeFile::new(format!("{static_dir_path}/index.html")),
    );

    // Election routes nested under /api/elections
    let elections_router = Router::new()
        .route(
            "/",
            get(list_elections).post(elections::handlers::create_election),
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
            "/{election_id}/tokens/{token_id}",
            get(ballot_tokens::handlers::get_token_info),
        )
        .route(
            "/{election_id}/tokens/{token_id}/redeem",
            post(ballot_tokens::handlers::redeem_token),
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
        .fallback_service(static_dir)
        .layer(cors))
}
