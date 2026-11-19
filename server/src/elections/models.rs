use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::counting::{GroupConfig, GroupedConfig, SystemConfig};

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "election_type", rename_all = "kebab-case")]
pub enum CreateElectionRequest {
    StvMd {
        #[schema(example = "Student Council Election 2026")]
        title: String,
        description: Option<String>,
        #[schema(example = json!(["Alice", "Bob", "Charlie"]))]
        candidates: Vec<String>,
        #[schema(example = 3)]
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
    },
    StvMdCoperland {
        title: String,
        description: Option<String>,
        #[schema(example = json!(["Alice", "Bob", "Charlie"]))]
        candidates: Vec<String>,
        #[schema(example = 3)]
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
    },
    StvMdGrouped {
        title: String,
        description: Option<String>,
        #[schema(example = json!(["Alice", "Bob", "Carol", "Dave"]))]
        candidates: Vec<String>,
        #[schema(example = 4)]
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        groups: Vec<GroupConfig>,
        candidate_groups: Vec<String>,
    },
    Schulze {
        title: String,
        description: Option<String>,
        #[schema(example = json!(["Alice", "Bob", "Charlie"]))]
        candidates: Vec<String>,
        #[schema(example = 1)]
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        /// Seed for the random tie-break lot. If omitted, the server
        /// generates one at creation time and persists it.
        #[serde(default)]
        tie_break_seed: Option<u64>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "election_type", rename_all = "kebab-case")]
pub enum ElectionResponse {
    StvMd {
        uuid: String,
        admin_uuid: String,
        title: String,
        description: Option<String>,
        candidates: Vec<String>,
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        is_locked: bool,
    },
    StvMdCoperland {
        uuid: String,
        admin_uuid: String,
        title: String,
        description: Option<String>,
        candidates: Vec<String>,
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        is_locked: bool,
    },
    StvMdGrouped {
        uuid: String,
        admin_uuid: String,
        title: String,
        description: Option<String>,
        candidates: Vec<String>,
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        is_locked: bool,
        groups: Vec<GroupConfig>,
        candidate_groups: Vec<String>,
    },
    Schulze {
        uuid: String,
        admin_uuid: String,
        title: String,
        description: Option<String>,
        candidates: Vec<String>,
        num_seats: u32,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        is_locked: bool,
        tie_break_seed: u64,
    },
}

impl CreateElectionRequest {
    pub fn election_type(&self) -> crate::counting::ElectionType {
        match self {
            CreateElectionRequest::StvMd { .. } => crate::counting::ElectionType::StvMd,
            CreateElectionRequest::StvMdCoperland { .. } => crate::counting::ElectionType::StvMdCoperland,
            CreateElectionRequest::StvMdGrouped { .. } => crate::counting::ElectionType::StvMdGrouped,
            CreateElectionRequest::Schulze { .. } => crate::counting::ElectionType::Schulze,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            CreateElectionRequest::StvMd { title, .. } => title,
            CreateElectionRequest::StvMdCoperland { title, .. } => title,
            CreateElectionRequest::StvMdGrouped { title, .. } => title,
            CreateElectionRequest::Schulze { title, .. } => title,
        }
    }

    pub fn description(&self) -> &Option<String> {
        match self {
            CreateElectionRequest::StvMd { description, .. } => description,
            CreateElectionRequest::StvMdCoperland { description, .. } => description,
            CreateElectionRequest::StvMdGrouped { description, .. } => description,
            CreateElectionRequest::Schulze { description, .. } => description,
        }
    }

    pub fn candidates(&self) -> &[String] {
        match self {
            CreateElectionRequest::StvMd { candidates, .. } => candidates,
            CreateElectionRequest::StvMdCoperland { candidates, .. } => candidates,
            CreateElectionRequest::StvMdGrouped { candidates, .. } => candidates,
            CreateElectionRequest::Schulze { candidates, .. } => candidates,
        }
    }

    pub fn num_seats(&self) -> u32 {
        match self {
            CreateElectionRequest::StvMd { num_seats, .. } => *num_seats,
            CreateElectionRequest::StvMdCoperland { num_seats, .. } => *num_seats,
            CreateElectionRequest::StvMdGrouped { num_seats, .. } => *num_seats,
            CreateElectionRequest::Schulze { num_seats, .. } => *num_seats,
        }
    }

    pub fn start_time(&self) -> &chrono::DateTime<chrono::Utc> {
        match self {
            CreateElectionRequest::StvMd { start_time, .. } => start_time,
            CreateElectionRequest::StvMdCoperland { start_time, .. } => start_time,
            CreateElectionRequest::StvMdGrouped { start_time, .. } => start_time,
            CreateElectionRequest::Schulze { start_time, .. } => start_time,
        }
    }

    pub fn end_time(&self) -> &chrono::DateTime<chrono::Utc> {
        match self {
            CreateElectionRequest::StvMd { end_time, .. } => end_time,
            CreateElectionRequest::StvMdCoperland { end_time, .. } => end_time,
            CreateElectionRequest::StvMdGrouped { end_time, .. } => end_time,
            CreateElectionRequest::Schulze { end_time, .. } => end_time,
        }
    }

    /// The system-specific config to persist in the `elections.config` column.
    ///
    /// `existing_tie_break_seed` is the seed of the currently stored config (if
    /// any): on update, an omitted seed preserves the existing one instead of
    /// generating a new one, so the config (and thus the lot) is stable for the
    /// life of the election.
    pub fn system_config(&self, existing_tie_break_seed: Option<u64>) -> Result<SystemConfig, String> {
        match self {
            CreateElectionRequest::StvMd { .. } => Ok(SystemConfig::StvMd {}),
            CreateElectionRequest::StvMdCoperland { .. } => Ok(SystemConfig::StvMdCoperland {}),
            CreateElectionRequest::StvMdGrouped { groups, candidate_groups, .. } => {
                Ok(SystemConfig::StvMdGrouped(GroupedConfig::try_new(
                    groups.clone(),
                    candidate_groups.clone(),
                )?))
            }
            CreateElectionRequest::Schulze { tie_break_seed, .. } => {
                Ok(SystemConfig::Schulze {
                    tie_break_seed: tie_break_seed
                        .or(existing_tie_break_seed)
                        .unwrap_or_else(rand::random),
                })
            }
        }
    }
}
