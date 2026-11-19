use std::collections::HashMap;

use super::pairwise::pairwise_matrix;
use super::{CombinedBallot, Elected, RankedElection, stv};

fn pairwise_order(
    ballots: &[CombinedBallot],
    n_candidates: usize,
) -> (HashMap<String, usize>, Vec<Vec<usize>>) {
    let matrix = pairwise_matrix(ballots, n_candidates);

    // Copeland score: number of pairwise wins
    let mut scores = vec![0; n_candidates];
    for i in 0..n_candidates {
        for j in 0..n_candidates {
            if i != j && matrix[i][j] > matrix[j][i] {
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

pub fn count(
    election: &RankedElection,
) -> Result<(crate::log::CountingLog, Vec<Elected>, HashMap<String, usize>, Vec<Vec<usize>>), String> {
    let candidates = election.candidates();
    let ballots = election.ballots();
    let outcome = stv::run(candidates, election.seats(), ballots);

    let (order, pairwise_matrix) = pairwise_order(ballots, candidates.len());

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
            candidate: candidates[e].clone(),
            id: e,
        })
        .collect();

    Ok((outcome.log, elected, order, pairwise_matrix))
}
