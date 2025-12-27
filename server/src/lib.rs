use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

pub mod counting;
pub mod db;
pub mod election_yaml;
pub mod error;

use election_yaml::ElectionConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: DbConn,
    pub elections_dir: String,
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
    let elections = election_yaml::load_elections(&state.elections_dir)
        .map_err(|e| error::Error::Internal(format!("Failed to load elections: {}", e)))?;
    Ok(Json(elections.values().cloned().collect()))
}

#[axum::debug_handler]
async fn get_election(
    Path(election_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ElectionState>, error::Error> {
    let election = election_yaml::load_election(&state.elections_dir, &election_id)
        .map_err(|_| error::Error::NotFound)?;

    // Fetch all ballots for this election (potential voters)
    let all_ballots = db::Entity::find()
        .filter(db::Column::ElectionId.eq(election_id.clone()))
        .all(&state.db)
        .await
        .map_err(|e| {
            error::Error::Internal(format!(
                "Failed to fetch ballots for {}: {}",
                election_id, e
            ))
        })?;

    let potential_voters = all_ballots.len();

    // Separate cast ballots (with content)
    let ballots: Vec<counting::Ballot> = all_ballots
        .iter()
        .filter_map(|b| {
            b.ballot_content.as_ref().and_then(|content| {
                serde_json::from_value::<counting::Ballot>(content.clone()).ok()
            })
        })
        .collect();

    let casted = ballots.len();

    // Only publish results after the election has ended
    let now = Utc::now();
    let results = if now >= election.end_time && casted > 0 {
        let election = counting::to_election(&election, &ballots);
        Some(counting::stv_droop(election).map_err(|e| {
            error::Error::Internal(format!("STV counting failed for {}: {:?}", election_id, e))
        })?)
    } else {
        None
    };

    Ok(Json(ElectionState {
        election: election.into_public(Utc::now()),
        potential_voters,
        casted,
        results,
    }))
}

#[axum::debug_handler]
async fn get_ballot(
    Path((election_id, uuid)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<Option<counting::Ballot>>, error::Error> {
    // Load election to validate it exists
    let election = election_yaml::load_election(&state.elections_dir, &election_id)
        .map_err(|_| error::Error::NotFound)?;

    // Validate ballot UUID is in the valid list
    if !election.ballots.contains(&uuid) {
        return Err(error::Error::NotFound);
    }

    let ballot = db::Entity::find_by_id(uuid.clone())
        .one(&state.db)
        .await
        .map_err(|e| error::Error::Internal(format!("Failed to fetch ballot {}: {}", uuid, e)))?;

    let Some(ballot) = ballot else {
        return Ok(Json(None));
    };

    if ballot.election_id != election_id {
        return Err(error::Error::NotFound);
    }

    let submission = match ballot.ballot_content {
        Some(content) => Some(serde_json::from_value(content).map_err(|e| {
            error::Error::Internal(format!(
                "Failed to parse ballot content for {}: {}",
                uuid, e
            ))
        })?),
        None => None,
    };

    Ok(Json(submission))
}

#[axum::debug_handler]
async fn put_ballot(
    Path((election_id, uuid)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(ballot): Json<counting::Ballot>,
) -> Result<Json<serde_json::Value>, error::Error> {
    // Load election and validate it exists
    let election = election_yaml::load_election(&state.elections_dir, &election_id)
        .map_err(|_| error::Error::NotFound)?;

    // Validate ballot UUID is in the valid list
    if !election.ballots.contains(&uuid) {
        return Err(error::Error::BadRequest(
            "Invalid ballot UUID for this election".to_string(),
        ));
    }

    // Validate ranks
    if ballot
        .ranks
        .iter()
        .filter_map(|&c| c)
        .any(|c| c >= election.candidates.len())
    {
        return Err(error::Error::BadRequest(
            "Invalid candidate index".to_string(),
        ));
    }

    // Enforce voting period
    let now = Utc::now();
    if now < election.start_time {
        return Err(error::Error::BadRequest(
            "Voting has not started".to_string(),
        ));
    }
    if now >= election.end_time {
        return Err(error::Error::BadRequest("Voting has ended".to_string()));
    }

    // Store or update ballot (idempotent per uuid)
    let ballot_json = serde_json::to_value(&ballot).map_err(|e| {
        error::Error::Internal(format!(
            "Failed to serialize ballot {} for {}: {}",
            uuid, election_id, e
        ))
    })?;

    if let Some(existing) = db::Entity::find_by_id(uuid.clone())
        .one(&state.db)
        .await
        .map_err(|e| {
            error::Error::Internal(format!(
                "Failed to query existing ballot {} for {}: {}",
                uuid, election_id, e
            ))
        })?
    {
        if existing.election_id != election_id {
            return Err(error::Error::NotFound);
        }

        let mut active: db::ActiveModel = existing.into();
        active.ballot_content = sea_orm::Set(Some(ballot_json));
        active.update(&state.db).await.map_err(|e| {
            error::Error::Internal(format!(
                "Failed to update ballot {} for {}: {}",
                uuid, election_id, e
            ))
        })?;
    } else {
        let ballot = db::ActiveModel {
            id: sea_orm::Set(uuid.clone()),
            election_id: sea_orm::Set(election_id.clone()),
            ballot_content: sea_orm::Set(Some(ballot_json)),
        };

        ballot.insert(&state.db).await.map_err(|e| {
            error::Error::Internal(format!(
                "Failed to insert ballot {} for {}: {}",
                uuid, election_id, e
            ))
        })?;
    }
    Ok(Json(serde_json::json!("null")))
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

    counting::stv_droop(election)
        .map_err(|e| error::Error::Internal(format!("Simulation failed: {:?}", e)))
        .map(Json)
}

pub fn create_app(db: DbConn, dir: String) -> Result<Router<()>, String> {
    use axum::http::Method;
    use tower_http::cors::{Any, CorsLayer};

    // Verify the elections directory exists and is readable
    std::fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read elections directory: {}", e))?
        .next();

    let state = AppState {
        db,
        elections_dir: dir,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT])
        .allow_headers(Any);

    let static_dir_path =
        std::env::var("FRONTEND_STATIC_DIR").unwrap_or_else(|_| "app/static".to_string());
    let static_dir = tower_http::services::ServeDir::new(&static_dir_path).fallback(
        tower_http::services::ServeFile::new(format!("{static_dir_path}/index.html")),
    );

    Ok(Router::new()
        .route("/api/health", get(health))
        .route("/api/elections", get(list_elections))
        .route("/api/elections/{election_id}", get(get_election))
        .route(
            "/api/elections/{election_id}/ballot/{uuid}",
            get(get_ballot).put(put_ballot),
        )
        .route("/api/simulate", post(simulate))
        .with_state(state)
        .fallback_service(static_dir)
        .layer(cors))
}
