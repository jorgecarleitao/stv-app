//! Schulze method (beatpath) for single-winner elections.
//!
//! Pairwise defeats are measured by their strength (number of ballots
//! preferring the winner of the pair), and the strength of the strongest path
//! `p[a][b]` is computed with the usual widest-path pass. Candidate `a` beats
//! candidate `b` iff `p[a][b] > p[b][a]`. The winner is the candidate who beats
//! every other candidate; if several candidates are mutually tied (equal
//! path strengths both ways), the winner is drawn by a random lot seeded with
//! the election's `tie_break_seed`, so the draw is reproducible and auditable.

use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::SeedableRng;

use super::pairwise::pairwise_matrix;
use super::{CombinedBallot, Elected, SchulzeElection};
use crate::log::{
    CountingLog, CountingLogAction, CountingLogActionType, CountingLogCandidate,
    CountingLogCandidateCount, CountingLogCandidateStatus, CountingLogHeader, CountingLogRound,
    CountingLogStats,
};

/// Strength of the strongest path from `a` to `b` in the defeat graph, where a
/// direct defeat `a > b` (more ballots prefer `a`) has strength `m[a][b]`.
pub fn strongest_paths(matrix: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = matrix.len();
    let mut p = vec![vec![0usize; n]; n];
    for (i, prow) in p.iter_mut().enumerate() {
        for (j, cell) in prow.iter_mut().enumerate() {
            if i != j && matrix[i][j] > matrix[j][i] {
                *cell = matrix[i][j];
            }
        }
    }
    for k in 0..n {
        let col_k: Vec<usize> = (0..n).map(|i| p[i][k]).collect();
        let row_k = p[k].clone();
        for (i, prow) in p.iter_mut().enumerate() {
            for (j, cell) in prow.iter_mut().enumerate() {
                let via = col_k[i].min(row_k[j]);
                if via > *cell {
                    *cell = via;
                }
            }
        }
    }
    p
}

/// Candidates that beat (or tie) every other candidate via strongest paths.
/// Two candidates in this set necessarily tie each other (`p[a][b] == p[b][a]`).
fn winner_set(strengths: &[Vec<usize>]) -> Vec<usize> {
    let n = strengths.len();
    (0..n)
        .filter(|&a| (0..n).all(|b| b == a || strengths[a][b] >= strengths[b][a]))
        .collect()
}

pub type SchulzeOutcome = (
    CountingLog,
    Vec<Elected>,
    Vec<Vec<usize>>,
    Vec<Vec<usize>>,
);

pub fn count(election: &SchulzeElection) -> Result<SchulzeOutcome, String> {
    let candidates = election.candidates();
    let n = candidates.len();
    let ballots = election.ballots();
    if n == 0 {
        return Err("Candidates list cannot be empty".to_string());
    }

    let matrix = pairwise_matrix(ballots, n);
    let strengths = strongest_paths(&matrix);
    let winners = winner_set(&strengths);
    if winners.is_empty() {
        // Unreachable for the Schulze beatpath order, but guard anyway.
        return Err("No winner could be determined".to_string());
    }

    let (winner_idx, tied) = if winners.len() == 1 {
        (winners[0], None)
    } else {
        let mut rng = StdRng::seed_from_u64(election.tie_break_seed());
        let chosen = *winners.choose(&mut rng).expect("winners is non-empty");
        (chosen, Some(winners))
    };

    let log = build_log(candidates, ballots, winner_idx, tied.as_deref(), election);
    let elected = vec![Elected {
        candidate: candidates[winner_idx].clone(),
        id: winner_idx,
    }];
    Ok((log, elected, matrix, strengths))
}

fn first_preference_tallies(ballots: &[CombinedBallot], n: usize) -> Vec<usize> {
    let mut tallies = vec![0usize; n];
    for ballot in ballots {
        for (cand, rank) in ballot.ranks.iter().enumerate() {
            if *rank == Some(0) {
                tallies[cand] += ballot.votes;
            }
        }
    }
    tallies
}

fn stats(total_votes: usize) -> CountingLogStats {
    // Schulze has no quota/surplus; those fields are zeroed for uniformity.
    CountingLogStats {
        quota: "0".to_string(),
        votes: total_votes.to_string(),
        residual: "0".to_string(),
        total: total_votes.to_string(),
        surplus: "0".to_string(),
    }
}

fn action(
    action_type: CountingLogActionType,
    statuses: &[CountingLogCandidateStatus],
    tallies: &[usize],
    total_votes: usize,
) -> CountingLogAction {
    CountingLogAction {
        action_type,
        candidate_counts: statuses
            .iter()
            .zip(tallies)
            .map(|(status, votes)| CountingLogCandidateCount {
                status: status.clone(),
                votes: votes.to_string(),
            })
            .collect(),
        stats: stats(total_votes),
    }
}

fn build_log(
    candidates: &[String],
    ballots: &[CombinedBallot],
    winner_idx: usize,
    tied: Option<&[usize]>,
    election: &SchulzeElection,
) -> CountingLog {
    let n = candidates.len();
    let total_votes: usize = ballots.iter().map(|b| b.votes).sum();
    let tallies = first_preference_tallies(ballots, n);

    let hopeful = vec![CountingLogCandidateStatus::Hopeful; n];
    let mut elected_statuses = hopeful.clone();
    elected_statuses[winner_idx] = CountingLogCandidateStatus::Elected;

    let mut actions = vec![action(
        CountingLogActionType::BeginCount,
        &hopeful,
        &tallies,
        total_votes,
    )];

    let by_lot = tied.is_some();
    if let Some(tied) = tied {
        actions.push(action(
            CountingLogActionType::Lot {
                candidates: tied.iter().map(|&i| candidates[i].clone()).collect(),
                winner: candidates[winner_idx].clone(),
            },
            &hopeful,
            &tallies,
            total_votes,
        ));
    }

    actions.push(action(
        CountingLogActionType::Elect {
            candidate: candidates[winner_idx].clone(),
            reason: if by_lot { "lot" } else { "beatpath" }.to_string(),
        },
        &elected_statuses,
        &tallies,
        total_votes,
    ));
    actions.push(action(
        CountingLogActionType::CountComplete,
        &elected_statuses,
        &tallies,
        total_votes,
    ));

    CountingLog {
        header: CountingLogHeader {
            title: String::new(),
            rule: "Schulze".to_string(),
            arithmetic: "usize".to_string(),
            seats: election.seats(),
            ballots: total_votes,
            quota: String::new(),
        },
        candidates: candidates
            .iter()
            .map(|name| CountingLogCandidate {
                name: name.clone(),
                withdrawn: false,
            })
            .collect(),
        rounds: vec![CountingLogRound {
            round_number: 0,
            actions,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counting::CombinedBallot;

    fn ballots(data: &[(usize, &[Option<usize>])]) -> Vec<CombinedBallot> {
        data.iter()
            .map(|(votes, ranks)| CombinedBallot {
                votes: *votes,
                ranks: ranks.to_vec(),
            })
            .collect()
    }

    #[test]
    fn pairwise_matrix_counts_ranked_over_unranked() {
        // candidates: A, B, C; one ballot A > B, C unranked
        let bs = ballots(&[(1, &[Some(0), Some(1), None])]);
        let m = pairwise_matrix(&bs, 3);
        assert_eq!(m[0][1], 1);
        assert_eq!(m[1][0], 0);
        assert_eq!(m[0][2], 1); // ranked beats unranked
        assert_eq!(m[2][0], 0);
        assert_eq!(m[1][2], 1);
        assert_eq!(m[2][1], 0);
    }

    #[test]
    fn schulze_resolves_cycle_by_strongest_path() {
        // 3x A>B>C, 2x B>C>A, 2x C>A>B: pairwise cycle A>B, B>C, C>A,
        // but the strongest path A>B>C (5) beats C>A (4), so A wins.
        let bs = ballots(&[
            (3, &[Some(0), Some(1), Some(2)]),
            (2, &[Some(2), Some(0), Some(1)]),
            (2, &[Some(1), Some(2), Some(0)]),
        ]);
        let m = pairwise_matrix(&bs, 3);
        let p = strongest_paths(&m);
        assert_eq!(winner_set(&p), vec![0]); // A
    }

    #[test]
    fn symmetric_tie_goes_to_seeded_lot() {
        // 1x A>B, 1x B>A: perfect tie, both in the winner set.
        let bs = ballots(&[(1, &[Some(0), Some(1)]), (1, &[Some(1), Some(0)])]);
        let m = pairwise_matrix(&bs, 2);
        let p = strongest_paths(&m);
        let winners = winner_set(&p);
        assert_eq!(winners.len(), 2);
        // Same seed => same draw, different seeds may differ; just assert determinism here.
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let a = *winners.choose(&mut rng1).unwrap();
        let b = *winners.choose(&mut rng2).unwrap();
        assert_eq!(a, b);
    }
}
