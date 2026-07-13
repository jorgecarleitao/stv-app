use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Ballot {
    /// Ranked preferences as indices into the candidates list. None indicates no preference for that position.
    #[schema(example = json!([0, 2, 1]))]
    pub ranks: Vec<Option<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CombinedBallot {
    /// Number of identical ballots with these rankings
    pub votes: usize,
    /// Ranked preferences as indices into the candidates list
    pub ranks: Vec<Option<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Election {
    /// List of candidate names
    pub candidates: Vec<String>,
    /// Number of seats to be filled
    pub seats: usize,
    /// Whether the order of elected candidates matters
    pub ordered_seats: bool,
    /// List of ballots (may be combined for identical rankings)
    pub ballots: Vec<CombinedBallot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Elected {
    /// Name of the elected candidate
    pub candidate: String,
    /// Index/ID of the candidate in the candidates list
    pub id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ElectionResult {
    /// The election that was counted
    pub election: Election,
    /// Detailed log of the counting process, parsed into structured data
    pub log: crate::log::CountingLog,
    /// List of elected candidates in order
    pub elected: Vec<Elected>,
    /// Map from candidate ID to their final position/order
    pub order: HashMap<usize, usize>,
    /// Pairwise comparison matrix showing head-to-head preferences between candidates
    pub pairwise_matrix: Vec<Vec<usize>>,
}

fn ranks_to_order(ranks: &[Option<usize>]) -> Vec<Vec<usize>> {
    ranks
        .iter()
        .enumerate()
        .filter_map(|(cand, &rank)| rank.map(|r| (r, cand)))
        .fold(vec![], |mut order, (rank, cand)| {
            while order.len() <= rank {
                order.push(vec![]);
            }
            order[rank].push(cand);
            order
        })
}

fn pairwise_order(
    ballots: &[CombinedBallot],
    n_candidates: usize,
) -> (HashMap<usize, usize>, Vec<Vec<usize>>) {
    // Precompute rank arrays for each ballot
    let ballot_ranks: Vec<Vec<Option<usize>>> = ballots.iter().map(|b| b.ranks.clone()).collect();

    let mut scores = vec![0; n_candidates];
    let mut matrix = vec![vec![0; n_candidates]; n_candidates];

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
            matrix[i][j] = i_beats_j_total;
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
    (result, matrix)
}

pub fn stv_droop(election: Election, ordered_seats: bool) -> Result<ElectionResult, String> {
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
        .map(|b| Ballot::new(b.votes, ranks_to_order(&b.ranks)))
        .collect::<Vec<_>>();

    let stv_election = Election::builder()
        .title("")
        .candidates(candidates)
        .num_seats(election.seats)
        .ballots(ballots)
        .build();

    let mut result = stv_rs::meek::stv_droop::<
        stv_rs::arithmetic::Integer64,
        stv_rs::arithmetic::FixedDecimal9,
    >(
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

    // Only perform pairwise ordering if ordered_seats is true
    let (order, pairwise_matrix) = if ordered_seats {
        let (ord, mat) = pairwise_order(&election.ballots, election.candidates.len());
        result
            .elected
            .sort_by_key(|&candidate_id| ord.get(&candidate_id).copied().unwrap_or(usize::MAX));
        (ord, mat)
    } else {
        // For unordered seats, use empty order and matrix
        (
            HashMap::new(),
            vec![vec![0; election.candidates.len()]; election.candidates.len()],
        )
    };

    let elected = result
        .elected
        .into_iter()
        .map(|e| Elected {
            candidate: election.candidates[e].clone(),
            id: e,
        })
        .collect();

    Ok(ElectionResult {
        election,
        log: crate::log::parse_log(&String::from_utf8(log).unwrap()),
        elected,
        order,
        pairwise_matrix,
    })
}

pub fn to_election(
    candidates: Vec<String>,
    seats: usize,
    ordered_seats: bool,
    ballots: Vec<Ballot>,
) -> Election {
    // Aggregate ballots by their ranked_choices pattern
    let mut pattern_counts: HashMap<Vec<Option<usize>>, usize> = HashMap::new();
    for ballot in ballots {
        *pattern_counts.entry(ballot.ranks).or_insert(0) += 1;
    }

    let mut ballots: Vec<CombinedBallot> = pattern_counts
        .into_iter()
        .map(|(ranks, votes)| CombinedBallot { votes, ranks })
        .collect();

    ballots.sort_by(|a, b| b.votes.cmp(&a.votes));

    Election {
        candidates: candidates.to_vec(),
        seats,
        ordered_seats,
        ballots,
    }
}
