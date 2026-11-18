use std::collections::HashMap;

use super::{CombinedBallot, Elected, Election, ElectionResult, stv};

fn pairwise_order(
    ballots: &[CombinedBallot],
    n_candidates: usize,
) -> (HashMap<String, usize>, Vec<Vec<usize>>) {
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
            }
            matrix[i][j] = i_beats_j_total;
            if i_beats_j_total > j_beats_i_total {
                scores[i] += 1;
            }
        }
    }

    let mut idxs: Vec<usize> = (0..n_candidates).collect();
    idxs.sort_by_key(|&i| std::cmp::Reverse(scores[i]));

    let mut result = HashMap::new();
    for (order, cand) in idxs.into_iter().enumerate() {
        result.insert(cand.to_string(), order);
    }
    (result, matrix)
}

pub fn count(election: Election) -> Result<ElectionResult, String> {
    let outcome = stv::run(election.candidates(), election.seats(), election.ballots());

    let (order, pairwise_matrix) = pairwise_order(election.ballots(), election.candidates().len());

    let mut elected_ids = outcome.elected;
    elected_ids.sort_by_key(|&candidate_id| {
        order
            .get(&candidate_id.to_string())
            .copied()
            .unwrap_or(usize::MAX)
    });

    let elected = elected_ids
        .into_iter()
        .map(|e| Elected {
            candidate: election.candidates()[e].clone(),
            id: e,
        })
        .collect();

    Ok(ElectionResult::StvMdCoperland {
        election,
        log: outcome.log,
        elected,
        order,
        pairwise_matrix,
    })
}
