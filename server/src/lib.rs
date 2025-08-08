use std::collections::HashMap;

use axum::{
    Json, Router, extract,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

pub mod error;

#[derive(Clone, axum::extract::FromRef)]
pub struct AppState {}

#[axum::debug_handler]
async fn health() -> Result<(), error::Error> {
    Ok(())
}

#[derive(Deserialize)]
pub struct Ballot {
    votes: usize,
    order: Vec<Vec<usize>>,
}

#[derive(Deserialize)]
pub struct Election {
    pub candidates: Vec<String>,
    pub seats: usize,
    pub ballots: Vec<Ballot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Elected {
    pub candidate: String,
    pub id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionResult {
    pub log: String,
    pub elected: Vec<Elected>,
}

fn order_to_ranks(order: &Vec<Vec<usize>>, n_candidates: usize) -> Vec<Option<usize>> {
    let mut ranks = vec![None; n_candidates];
    for (rank, group) in order.iter().enumerate() {
        for &cand in group {
            ranks[cand] = Some(rank + 1);
        }
    }
    ranks
}

pub fn pairwise_order(ballots: &[Ballot], n_candidates: usize) -> HashMap<usize, usize> {
    // Precompute rank arrays for each ballot
    let ballot_ranks: Vec<Vec<Option<usize>>> = ballots
        .iter()
        .map(|b| order_to_ranks(&b.order, n_candidates))
        .collect();

    let mut scores = vec![0; n_candidates];

    for i in 0..n_candidates {
        for j in 0..n_candidates {
            if i == j {
                continue;
            }
            let mut i_beats_j_total = 0;
            let mut j_beats_i_total = 0;

            for (ranks, ballot) in ballot_ranks.iter().zip(ballots.iter()) {
                let r_i = ranks[i];
                let r_j = ranks[j];
                if let (Some(r_i), Some(r_j)) = (r_i, r_j) {
                    if r_i < r_j {
                        i_beats_j_total += ballot.votes;
                    } else if r_j < r_i {
                        j_beats_i_total += ballot.votes;
                    }
                } else if let (Some(_), None) = (r_i, r_j) {
                    i_beats_j_total += ballot.votes;
                } else if let (None, Some(_)) = (r_i, r_j) {
                    j_beats_i_total += ballot.votes;
                }
                // both None = tie, ignore
            }
            if i_beats_j_total > j_beats_i_total {
                scores[i] += 1;
            }
        }
    }

    // Sort: best score gets 0
    let mut idxs: Vec<usize> = (0..n_candidates).collect();
    idxs.sort_by_key(|&i| std::cmp::Reverse(scores[i]));

    // Map candidate id to order
    let mut result = HashMap::new();
    for (order, cand) in idxs.into_iter().enumerate() {
        result.insert(cand, order);
    }
    result
}

#[axum::debug_handler]
async fn stv_droop(
    extract::Json(election): extract::Json<Election>,
) -> Result<Json<ElectionResult>, String> {
    use num::{BigInt, BigRational};
    use stv_rs::types::*;

    let mut log = Vec::new();

    let candidates = election
        .candidates
        .iter()
        .map(|c| Candidate::new(c, false))
        .collect::<Vec<_>>();

    let ballots = election
        .ballots
        .iter()
        .map(|b| Ballot::new(b.votes, b.order.clone()))
        .collect::<Vec<_>>();

    let stv_election = Election::builder()
        .title("")
        .candidates(candidates)
        .num_seats(election.seats)
        .ballots(ballots)
        .build();

    let mut result = stv_rs::meek::stv_droop::<BigInt, BigRational>(
        &mut log,
        &stv_election,
        "election",
        6,
        stv_rs::cli::Parallel::No,
        None,
        true,
        false,
        true,
    )
    .unwrap();

    let order = pairwise_order(&election.ballots, election.candidates.len());
    result
        .elected
        .sort_by_key(|&candidate_id| order.get(&candidate_id).copied().unwrap_or(usize::MAX));

    Ok(Json(crate::ElectionResult {
        log: String::from_utf8(log).unwrap(),
        elected: result
            .elected
            .into_iter()
            .map(|e| Elected {
                candidate: election.candidates[e].clone(),
                id: e,
            })
            .collect(),
    }))
}

pub async fn create_app(state: AppState) -> Result<Router<()>, String> {
    use axum::http::Method;
    use tower_http::cors::{Any, CorsLayer};
    let cors = CorsLayer::new()
        .allow_origin(Any) // or use .allow_origin("http://localhost:5173".parse().unwrap()) for strict dev settings
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let static_dir_path =
        std::env::var("FRONTEND_STATIC_DIR").unwrap_or_else(|_| "app/static".to_string());
    let static_dir = tower_http::services::ServeDir::new(&static_dir_path).fallback(
        tower_http::services::ServeFile::new(format!("{static_dir_path}/index.html")),
    );
    Ok(Router::new()
        .route("/api/health", get(health))
        .route("/api/election", post(stv_droop))
        .with_state(state)
        .fallback_service(static_dir)
        .layer(cors))
}
