use super::CombinedBallot;

/// Pairwise comparison matrix: `m[i][j]` is the total ballot weight that
/// prefers candidate `i` over candidate `j`.
///
/// A ranked candidate is preferred over an unranked one; equal ranks, or both
/// unranked, count for neither side.
pub fn pairwise_matrix(ballots: &[CombinedBallot], n_candidates: usize) -> Vec<Vec<usize>> {
    let mut matrix = vec![vec![0usize; n_candidates]; n_candidates];
    for (i, row) in matrix.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            if i == j {
                continue;
            }
            let mut i_over_j = 0usize;
            for ballot in ballots {
                match (ballot.ranks[i], ballot.ranks[j]) {
                    (Some(ri), Some(rj)) => {
                        if ri < rj {
                            i_over_j += ballot.votes;
                        }
                    }
                    (Some(_), None) => i_over_j += ballot.votes,
                    _ => {}
                }
            }
            *cell = i_over_j;
        }
    }
    matrix
}
