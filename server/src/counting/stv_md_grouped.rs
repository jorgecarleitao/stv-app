use super::{CombinedBallot, Elected, Election, GroupResult, RankedElection, StvMdGroupedElection, stv};

/// Count a grouped election. The `StvMdGroupedElection` type guarantees the
/// grouped invariants (validated at construction), so no runtime checks are
/// needed here.
pub fn count(election: &StvMdGroupedElection) -> Result<(Vec<GroupResult>, Vec<Elected>), String> {
    let candidates = election.candidates();
    let ballots = election.ballots();
    let groups = election.groups();
    let candidate_groups = election.candidate_groups();

    let mut group_results = Vec::new();
    let mut all_elected = Vec::new();

    for group in groups {
        let group_name = group.name();
        let group_seats = group.seats() as usize;

        let group_indices: Vec<usize> = candidate_groups
            .iter()
            .enumerate()
            .filter(|(_, g)| *g == group_name)
            .map(|(i, _)| i)
            .collect();

        let group_candidates: Vec<String> = group_indices
            .iter()
            .map(|&i| candidates[i].clone())
            .collect();

        let group_ballots: Vec<CombinedBallot> = ballots
            .iter()
            .map(|b| {
                let local_ranks: Vec<(usize, Option<usize>)> = group_indices
                    .iter()
                    .enumerate()
                    .map(|(local, &orig_idx)| {
                        let rank = if orig_idx < b.ranks.len() {
                            b.ranks[orig_idx]
                        } else {
                            None
                        };
                        (local, rank)
                    })
                    .collect();

                let mut ranked_pairs: Vec<(usize, usize)> = local_ranks
                    .iter()
                    .enumerate()
                    .filter_map(|(local, &(_, rank))| rank.map(|r| (local, r)))
                    .collect();
                ranked_pairs.sort_by_key(|&(_, r)| r);

                let mut new_ranks = vec![None; group_indices.len()];
                for (new_rank, (local, _)) in ranked_pairs.iter().enumerate() {
                    new_ranks[*local] = Some(new_rank);
                }

                CombinedBallot {
                    votes: b.votes,
                    ranks: new_ranks,
                }
            })
            .collect();

        // Sub-election for this group. Built directly (not via `new`): the
        // parent election's validation already guarantees well-formed data.
        let group_election = Election::StvMd(RankedElection {
            candidates: group_candidates.clone(),
            seats: group_seats,
            ballots: group_ballots.clone(),
        });

        let outcome = stv::run(&group_candidates, group_seats, &group_ballots);

        let mapped_elected: Vec<Elected> = outcome
            .elected
            .iter()
            .map(|&e| {
                let original_id = group_indices[e];
                Elected {
                    candidate: candidates[original_id].clone(),
                    id: original_id,
                }
            })
            .collect();

        let group_result = GroupResult {
            group: group_name.to_string(),
            seats: group_seats,
            election: group_election,
            log: outcome.log,
            elected: mapped_elected.clone(),
        };

        all_elected.extend(mapped_elected);
        group_results.push(group_result);
    }

    Ok((group_results, all_elected))
}
