use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod counting;
pub mod db;
pub mod election_yaml;
pub mod error;

use election_yaml::ElectionConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: DbConn,
    pub elections: HashMap<String, ElectionConfig>,
}

#[derive(Debug, Serialize)]
pub struct ElectionState {
    pub election: ElectionConfig,
    pub potential_voters: usize,
    pub casted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<counting::ElectionResult>,
}

#[derive(Debug, Deserialize)]
pub struct CastBallotRequest {
    pub ranked_choices: Vec<Option<usize>>,
}

#[axum::debug_handler]
async fn health() -> Result<(), error::Error> {
    Ok(())
}

#[axum::debug_handler]
async fn list_elections(
    State(state): State<AppState>,
) -> Result<Json<Vec<ElectionConfig>>, error::Error> {
    Ok(Json(state.elections.values().cloned().collect()))
}

#[axum::debug_handler]
async fn get_election(
    Path(election_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ElectionState>, error::Error> {
    let election = state
        .elections
        .get(&election_id)
        .ok_or(error::Error::NotFound)?
        .clone();

    // Fetch all ballots for this election (potential voters)
    let all_ballots = db::Entity::find()
        .filter(db::Column::ElectionId.eq(election_id.clone()))
        .all(&state.db)
        .await
        .map_err(|_| error::Error::Internal)?;

    let potential_voters = all_ballots.len();

    // Separate cast ballots (with content)
    let ballots: Vec<election_yaml::BallotSubmission> = all_ballots
        .iter()
        .filter_map(|b| {
            b.ballot_content.as_ref().and_then(|content| {
                serde_json::from_value::<election_yaml::BallotSubmission>(content.clone()).ok()
            })
        })
        .collect();

    let casted = ballots.len();

    // Compute results if there are ballots
    let results = if casted > 0 {
        Some(counting::compute_results(&election, &ballots).map_err(|_| error::Error::Internal)?)
    } else {
        None
    };

    Ok(Json(ElectionState {
        election: election.clone(),
        potential_voters,
        casted,
        results,
    }))
}

#[axum::debug_handler]
async fn get_ballot(
    Path((election_id, uuid)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<Option<election_yaml::BallotSubmission>>), error::Error> {
    state
        .elections
        .get(&election_id)
        .ok_or(error::Error::NotFound)?;

    let ballot = db::Entity::find_by_id(uuid.clone())
        .one(&state.db)
        .await
        .map_err(|_| error::Error::Internal)?
        .ok_or(error::Error::NotFound)?;

    if ballot.election_id != election_id {
        return Err(error::Error::NotFound);
    }

    let submission = match ballot.ballot_content {
        Some(content) => Some(serde_json::from_value(content).map_err(|_| error::Error::Internal)?),
        None => None,
    };

    Ok((StatusCode::OK, Json(submission)))
}

#[axum::debug_handler]
async fn put_ballot(
    Path((election_id, uuid)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(req): Json<CastBallotRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), error::Error> {
    // Validate election exists
    let election = state
        .elections
        .get(&election_id)
        .ok_or(error::Error::NotFound)?;

    // Validate ranked_choices
    if req
        .ranked_choices
        .iter()
        .filter_map(|&c| c)
        .any(|c| c >= election.candidates.len())
    {
        return Err(error::Error::BadRequest(
            "Invalid candidate index".to_string(),
        ));
    }

    // Store or update ballot (idempotent per uuid)
    let ballot_json = serde_json::json!({
        "ranked_choices": req.ranked_choices,
    });

    if let Some(existing) = db::Entity::find_by_id(uuid.clone())
        .one(&state.db)
        .await
        .map_err(|_| error::Error::Internal)?
    {
        if existing.election_id != election_id {
            return Err(error::Error::NotFound);
        }

        let mut active: db::ActiveModel = existing.into();
        active.ballot_content = sea_orm::Set(Some(ballot_json));
        active
            .update(&state.db)
            .await
            .map_err(|_| error::Error::Internal)?;
    } else {
        let ballot = db::ActiveModel {
            id: sea_orm::Set(uuid),
            election_id: sea_orm::Set(election_id.clone()),
            ballot_content: sea_orm::Set(Some(ballot_json)),
            ..Default::default()
        };

        ballot
            .insert(&state.db)
            .await
            .map_err(|_| error::Error::Internal)?;
    }
    Ok((StatusCode::OK, Json(serde_json::json!("null"))))
}

pub fn create_app(db: DbConn, elections_dir: &str) -> Result<Router<()>, String> {
    use axum::http::Method;
    use tower_http::cors::{Any, CorsLayer};

    // Load elections from YAML files
    let elections = election_yaml::load_elections(elections_dir)
        .map_err(|e| format!("Failed to load elections: {}", e))?;

    let state = AppState { db, elections };

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
        .route("/api/elections/:election_id", get(get_election))
        .route(
            "/api/elections/:election_id/ballot/:uuid",
            get(get_ballot).put(put_ballot),
        )
        .with_state(state)
        .fallback_service(static_dir)
        .layer(cors))
}
