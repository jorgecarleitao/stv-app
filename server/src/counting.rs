use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ElectionType {
    StvMd,
    StvMdCoperland,
    StvMdGrouped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupConfig {
    pub name: String,
    pub seats: u32,
}

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
#[serde(tag = "election_type", rename_all = "kebab-case")]
pub enum Election {
    StvMd {
        candidates: Vec<String>,
        seats: usize,
        ballots: Vec<CombinedBallot>,
    },
    StvMdCoperland {
        candidates: Vec<String>,
        seats: usize,
        ballots: Vec<CombinedBallot>,
    },
    StvMdGrouped {
        candidates: Vec<String>,
        seats: usize,
        ballots: Vec<CombinedBallot>,
        groups: Vec<GroupConfig>,
        candidate_groups: Vec<String>,
    },
}

impl Election {
    pub fn candidates(&self) -> &[String] {
        match self {
            Election::StvMd { candidates, .. } => candidates,
            Election::StvMdCoperland { candidates, .. } => candidates,
            Election::StvMdGrouped { candidates, .. } => candidates,
        }
    }
    pub fn seats(&self) -> usize {
        match self {
            Election::StvMd { seats, .. } => *seats,
            Election::StvMdCoperland { seats, .. } => *seats,
            Election::StvMdGrouped { seats, .. } => *seats,
        }
    }
    pub fn ballots(&self) -> &[CombinedBallot] {
        match self {
            Election::StvMd { ballots, .. } => ballots,
            Election::StvMdCoperland { ballots, .. } => ballots,
            Election::StvMdGrouped { ballots, .. } => ballots,
        }
    }
    pub fn election_type(&self) -> ElectionType {
        match self {
            Election::StvMd { .. } => ElectionType::StvMd,
            Election::StvMdCoperland { .. } => ElectionType::StvMdCoperland,
            Election::StvMdGrouped { .. } => ElectionType::StvMdGrouped,
        }
    }
    pub fn groups(&self) -> &[GroupConfig] {
        match self {
            Election::StvMdGrouped { groups, .. } => groups,
            _ => &[],
        }
    }
    pub fn candidate_groups(&self) -> &[String] {
        match self {
            Election::StvMdGrouped { candidate_groups, .. } => candidate_groups,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Elected {
    /// Name of the elected candidate
    pub candidate: String,
    /// Index/ID of the candidate in the candidates list
    pub id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupResult {
    /// Name of the group
    pub group: String,
    /// Number of seats allocated to this group
    pub seats: usize,
    /// Sub-election for this group (filtered candidates and ballots)
    pub election: Election,
    /// Detailed counting log for this group
    pub log: crate::log::CountingLog,
    /// Candidates elected from this group
    pub elected: Vec<Elected>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ElectionResult {
    StvMd {
        election: Election,
        log: crate::log::CountingLog,
        elected: Vec<Elected>,
    },
    StvMdCoperland {
        election: Election,
        log: crate::log::CountingLog,
        elected: Vec<Elected>,
        order: HashMap<String, usize>,
        pairwise_matrix: Vec<Vec<usize>>,
    },
    StvMdGrouped {
        election: Election,
        groups: Vec<GroupConfig>,
        group_results: Vec<GroupResult>,
        elected: Vec<Elected>,
    },
}

impl ElectionResult {
    pub fn election(&self) -> &Election {
        match self {
            ElectionResult::StvMd { election, .. } => election,
            ElectionResult::StvMdCoperland { election, .. } => election,
            ElectionResult::StvMdGrouped { election, .. } => election,
        }
    }

    pub fn elected(&self) -> &[Elected] {
        match self {
            ElectionResult::StvMd { elected, .. } => elected,
            ElectionResult::StvMdCoperland { elected, .. } => elected,
            ElectionResult::StvMdGrouped { elected, .. } => elected,
        }
    }

    pub fn log(&self) -> &crate::log::CountingLog {
        match self {
            ElectionResult::StvMd { log, .. } => log,
            ElectionResult::StvMdCoperland { log, .. } => log,
            ElectionResult::StvMdGrouped { .. } => {
                panic!("log() is not supported for grouped results; use group_results() instead")
            }
        }
    }

    pub fn order(&self) -> Option<&HashMap<String, usize>> {
        match self {
            ElectionResult::StvMd { .. } => None,
            ElectionResult::StvMdCoperland { order, .. } => Some(order),
            ElectionResult::StvMdGrouped { .. } => None,
        }
    }

    pub fn pairwise_matrix(&self) -> Option<&Vec<Vec<usize>>> {
        match self {
            ElectionResult::StvMd { .. } => None,
            ElectionResult::StvMdCoperland { pairwise_matrix, .. } => Some(pairwise_matrix),
            ElectionResult::StvMdGrouped { .. } => None,
        }
    }

    pub fn group_results(&self) -> Option<&[GroupResult]> {
        match self {
            ElectionResult::StvMdGrouped { group_results, .. } => Some(group_results),
            _ => None,
        }
    }
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

pub fn stv_droop(election: Election) -> Result<ElectionResult, String> {
    match &election {
        Election::StvMdGrouped { .. } => stv_droop_grouped(election),
        _ => stv_droop_single(election),
    }
}

fn stv_droop_single(election: Election) -> Result<ElectionResult, String> {
    use num::BigInt;
    use stv_rs::arithmetic::BigFixedDecimal9;
    use stv_rs::types::{Ballot, Candidate, Election};

    let mut log = Vec::new();

    let candidates = election
        .candidates()
        .iter()
        .map(|c| Candidate::new(c, false))
        .collect::<Vec<_>>();

    let ballots = election
        .ballots()
        .iter()
        .map(|b| Ballot::new(b.votes, ranks_to_order(&b.ranks)))
        .collect::<Vec<_>>();

    let stv_election = Election::builder()
        .title("")
        .candidates(candidates)
        .num_seats(election.seats())
        .ballots(ballots)
        .build();

    let mut result = stv_rs::meek::stv_droop::<
        BigInt,
        BigFixedDecimal9,
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

    let mut counting_log = crate::log::parse_log(&String::from_utf8(log).unwrap());
    crate::log::sort_candidate_counts(&mut counting_log, election.candidates());

    match election.election_type() {
        ElectionType::StvMd => {
            let elected = result
                .elected
                .into_iter()
                .map(|e| Elected {
                    candidate: election.candidates()[e].clone(),
                    id: e,
                })
                .collect();

            Ok(ElectionResult::StvMd {
                election,
                log: counting_log,
                elected,
            })
        }
        ElectionType::StvMdCoperland => {
            let (order, pairwise_matrix) =
                pairwise_order(election.ballots(), election.candidates().len());
            result
                .elected
                .sort_by_key(|&candidate_id| order.get(&candidate_id.to_string()).copied().unwrap_or(usize::MAX));

            let elected = result
                .elected
                .into_iter()
                .map(|e| Elected {
                    candidate: election.candidates()[e].clone(),
                    id: e,
                })
                .collect();

            Ok(ElectionResult::StvMdCoperland {
                election,
                log: counting_log,
                elected,
                order,
                pairwise_matrix,
            })
        }
        ElectionType::StvMdGrouped => {
            unreachable!("grouped elections handled separately")
        }
    }
}

fn stv_droop_grouped(election: Election) -> Result<ElectionResult, String> {
    let candidates = election.candidates().to_vec();
    let ballots = election.ballots().to_vec();
    let groups = election.groups().to_vec();
    let candidate_groups = election.candidate_groups().to_vec();
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

        let group_indices: Vec<usize> = candidate_groups
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

        let group_election = Election::StvMd {
            candidates: group_candidates.clone(),
            seats: group_seats,
            ballots: group_ballots,
        };

        let sub_result = stv_droop_single(group_election)?;

        let sub_elected = sub_result.elected();
        let mapped_elected: Vec<Elected> = sub_elected
            .iter()
            .map(|e| {
                let original_id = group_indices[e.id];
                Elected {
                    candidate: candidates[original_id].clone(),
                    id: original_id,
                }
            })
            .collect();

        let group_result = GroupResult {
            group: group_name.clone(),
            seats: group_seats,
            election: sub_result.election().clone(),
            log: sub_result.log().clone(),
            elected: mapped_elected.clone(),
        };

        all_elected.extend(mapped_elected);
        group_results.push(group_result);
    }

    let groups = election.groups().to_vec();
    Ok(ElectionResult::StvMdGrouped {
        election,
        groups,
        group_results,
        elected: all_elected,
    })
}

pub fn to_election(
    candidates: Vec<String>,
    seats: usize,
    election_type: ElectionType,
    ballots: Vec<Ballot>,
) -> Election {
    to_election_with_groups(candidates, seats, election_type, ballots, vec![], vec![])
}

pub fn to_election_with_groups(
    candidates: Vec<String>,
    seats: usize,
    election_type: ElectionType,
    ballots: Vec<Ballot>,
    groups: Vec<GroupConfig>,
    candidate_groups: Vec<String>,
) -> Election {
    let mut pattern_counts: HashMap<Vec<Option<usize>>, usize> = HashMap::new();
    for ballot in ballots {
        *pattern_counts.entry(ballot.ranks).or_insert(0) += 1;
    }

    let mut ballots: Vec<CombinedBallot> = pattern_counts
        .into_iter()
        .map(|(ranks, votes)| CombinedBallot { votes, ranks })
        .collect();

    ballots.sort_by(|a, b| b.votes.cmp(&a.votes));

    match election_type {
        ElectionType::StvMd => Election::StvMd {
            candidates,
            seats,
            ballots,
        },
        ElectionType::StvMdCoperland => Election::StvMdCoperland {
            candidates,
            seats,
            ballots,
        },
        ElectionType::StvMdGrouped => Election::StvMdGrouped {
            candidates,
            seats,
            ballots,
            groups,
            candidate_groups,
        },
    }
}
