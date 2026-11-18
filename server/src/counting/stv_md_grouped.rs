use super::{CombinedBallot, Elected, Election, ElectionResult, GroupResult, stv};

pub fn count(election: Election) -> Result<ElectionResult, String> {
    let groups = election.groups().to_vec();
    let seats = election.seats();

    let total_group_seats: u32 = groups.iter().map(|g| g.seats).sum();
    if total_group_seats as usize != seats {
        return Err(format!(
            "Sum of group seats ({}) must equal total seats ({})",
            total_group_seats, seats
        ));
    }

    let mut group_results = Vec::new();
    let mut all_elected = Vec::new();

    for group in &groups {
        let group_name = &group.name;
        let group_seats = group.seats as usize;

        let group_indices: Vec<usize> = election
            .candidate_groups()
            .iter()
            .enumerate()
            .filter(|(_, g)| *g == group_name)
            .map(|(i, _)| i)
            .collect();

        if group_indices.is_empty() {
            return Err(format!("No candidates assigned to group '{}'", group_name));
        }

        let group_candidates: Vec<String> = group_indices
            .iter()
            .map(|&i| election.candidates()[i].clone())
            .collect();

        let group_ballots: Vec<CombinedBallot> = election
            .ballots()
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

        let group_election = Election::StvMd {
            candidates: group_candidates.clone(),
            seats: group_seats,
            ballots: group_ballots.clone(),
        };

        let outcome = stv::run(&group_candidates, group_seats, &group_ballots);

        let mapped_elected: Vec<Elected> = outcome
            .elected
            .iter()
            .map(|&e| {
                let original_id = group_indices[e];
                Elected {
                    candidate: election.candidates()[original_id].clone(),
                    id: original_id,
                }
            })
            .collect();

        let group_result = GroupResult {
            group: group_name.clone(),
            seats: group_seats,
            election: group_election,
            log: outcome.log,
            elected: mapped_elected.clone(),
        };

        all_elected.extend(mapped_elected);
        group_results.push(group_result);
    }

    Ok(ElectionResult::StvMdGrouped {
        election,
        groups,
        group_results,
        elected: all_elected,
    })
}
