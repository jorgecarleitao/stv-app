use std::collections::HashMap;

use crate::election_yaml::{BallotSubmission, ElectionConfig};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Ballot {
    pub votes: usize,
    pub order: Vec<Vec<usize>>,
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

fn pairwise_order(ballots: &[Ballot], n_candidates: usize) -> HashMap<usize, usize> {
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

fn stv_droop(election: Election) -> Result<ElectionResult, String> {
    use num::{BigInt, BigRational};
    use stv_rs::types::{Ballot, Candidate, Election};

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

    Ok(ElectionResult {
        log: String::from_utf8(log).unwrap(),
        elected: result
            .elected
            .into_iter()
            .map(|e| Elected {
                candidate: election.candidates[e].clone(),
                id: e,
            })
            .collect(),
    })
}

pub fn compute_results(
    election: &ElectionConfig,
    ballots: &[BallotSubmission],
) -> Result<ElectionResult, String> {
    // Aggregate ballots by their ranked_choices pattern
    let mut pattern_counts: HashMap<Vec<Option<usize>>, usize> = HashMap::new();
    for ballot in ballots {
        *pattern_counts
            .entry(ballot.ranked_choices.clone())
            .or_insert(0) += 1;
    }

    // Convert aggregated patterns to Ballot format
    let ballots: Vec<Ballot> = pattern_counts
        .into_iter()
        .map(|(ranked_choices, votes)| {
            let order = ranked_choices
                .iter()
                .enumerate()
                .filter_map(|(cand, &rank)| rank.map(|r| (r, cand)))
                .fold(vec![], |mut order, (rank, cand)| {
                    while order.len() <= rank {
                        order.push(vec![]);
                    }
                    order[rank].push(cand);
                    order
                });

            Ballot { votes, order }
        })
        .collect();

    let election = Election {
        candidates: election.candidates.clone(),
        seats: election.seats,
        ballots,
    };

    stv_droop(election)
}
