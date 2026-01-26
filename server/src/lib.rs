use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::Utc;
use sea_orm::{ColumnTrait, DbConn, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

pub mod ballot_tokens;
pub mod ballots;
pub mod counting;
pub mod elections;
pub mod error;

/// ElectionConfig - what gets sent to the API (no sensitive info)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionConfig {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub candidates: Vec<String>,
    pub seats: u32,
    pub ordered_seats: bool,
    pub start_time: chrono::DateTime<Utc>,
    pub end_time: chrono::DateTime<Utc>,
    pub number_of_ballots: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ballots: Option<Vec<String>>, // Could list all ballot IDs for transparency (voter knows their UUID)
}

#[derive(Clone)]
pub struct AppState {
    pub db: DbConn,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionState {
    pub election: ElectionConfig,
    pub potential_voters: usize,
    pub casted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<counting::ElectionResult>,
}

#[axum::debug_handler]
async fn health() -> Result<(), error::Error> {
    Ok(())
}

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
            ordered_seats: e.ordered_seats,
            start_time: e.start_time,
            end_time: e.end_time,
            number_of_ballots: 0, // Would need to query tokens to get accurate count
            ballots: None,
        })
        .collect();

    Ok(Json(configs))
}

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

    let results = if has_ended && casted > 0 {
        let election_for_counting = counting::to_election(
            election.candidates.0.clone(),
            election.num_seats as usize,
            election.ordered_seats,
            ballots,
        );
        Some(counting::stv_droop(election_for_counting, election.ordered_seats).map_err(|e| {
            error::Error::Internal(format!(
                "STV counting failed for {}: {:?}",
                election_uuid, e
            ))
        })?)
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
        ordered_seats: election.ordered_seats,
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

    counting::stv_droop(election.clone(), election.ordered_seats)
        .map_err(|e| error::Error::Internal(format!("Simulation failed: {:?}", e)))
        .map(Json)
}

pub fn create_app(db: DbConn) -> Result<Router<()>, String> {
    use axum::http::Method;
    use tower_http::cors::{Any, CorsLayer};

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
        .with_state(state)
        .fallback_service(static_dir)
        .layer(cors))
}
