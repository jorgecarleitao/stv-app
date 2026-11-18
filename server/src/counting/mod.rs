use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

mod stv;
pub mod stv_md;
pub mod stv_md_coperland;
pub mod stv_md_grouped;

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
pub struct RankedBallot {
    #[schema(example = json!([0, 2, 1]))]
    pub ranks: Vec<Option<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "election_type", rename_all = "kebab-case")]
pub enum CastBallotRequest {
    StvMd(RankedBallot),
    StvMdCoperland(RankedBallot),
    StvMdGrouped(RankedBallot),
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

}

pub fn count(election: Election) -> Result<ElectionResult, String> {
    match election.election_type() {
        ElectionType::StvMd => stv_md::count(election),
        ElectionType::StvMdCoperland => stv_md_coperland::count(election),
        ElectionType::StvMdGrouped => stv_md_grouped::count(election),
    }
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
