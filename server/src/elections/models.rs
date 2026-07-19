use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::counting::GroupConfig;

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
}

impl CreateElectionRequest {
    pub fn election_type(&self) -> crate::counting::ElectionType {
        match self {
            CreateElectionRequest::StvMd { .. } => crate::counting::ElectionType::StvMd,
            CreateElectionRequest::StvMdCoperland { .. } => crate::counting::ElectionType::StvMdCoperland,
            CreateElectionRequest::StvMdGrouped { .. } => crate::counting::ElectionType::StvMdGrouped,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            CreateElectionRequest::StvMd { title, .. } => title,
            CreateElectionRequest::StvMdCoperland { title, .. } => title,
            CreateElectionRequest::StvMdGrouped { title, .. } => title,
        }
    }

    pub fn description(&self) -> &Option<String> {
        match self {
            CreateElectionRequest::StvMd { description, .. } => description,
            CreateElectionRequest::StvMdCoperland { description, .. } => description,
            CreateElectionRequest::StvMdGrouped { description, .. } => description,
        }
    }

    pub fn candidates(&self) -> &[String] {
        match self {
            CreateElectionRequest::StvMd { candidates, .. } => candidates,
            CreateElectionRequest::StvMdCoperland { candidates, .. } => candidates,
            CreateElectionRequest::StvMdGrouped { candidates, .. } => candidates,
        }
    }

    pub fn num_seats(&self) -> u32 {
        match self {
            CreateElectionRequest::StvMd { num_seats, .. } => *num_seats,
            CreateElectionRequest::StvMdCoperland { num_seats, .. } => *num_seats,
            CreateElectionRequest::StvMdGrouped { num_seats, .. } => *num_seats,
        }
    }

    pub fn start_time(&self) -> &chrono::DateTime<chrono::Utc> {
        match self {
            CreateElectionRequest::StvMd { start_time, .. } => start_time,
            CreateElectionRequest::StvMdCoperland { start_time, .. } => start_time,
            CreateElectionRequest::StvMdGrouped { start_time, .. } => start_time,
        }
    }

    pub fn end_time(&self) -> &chrono::DateTime<chrono::Utc> {
        match self {
            CreateElectionRequest::StvMd { end_time, .. } => end_time,
            CreateElectionRequest::StvMdCoperland { end_time, .. } => end_time,
            CreateElectionRequest::StvMdGrouped { end_time, .. } => end_time,
        }
    }

    pub fn groups(&self) -> &[GroupConfig] {
        match self {
            CreateElectionRequest::StvMdGrouped { groups, .. } => groups,
            _ => &[],
        }
    }

    pub fn candidate_groups(&self) -> &[String] {
        match self {
            CreateElectionRequest::StvMdGrouped { candidate_groups, .. } => candidate_groups,
            _ => &[],
        }
    }
}
