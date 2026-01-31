use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateElectionRequest {
    /// Title of the election
    #[schema(example = "Student Council Election 2026")]
    pub title: String,
    /// Optional description providing additional details about the election
    pub description: Option<String>,
    /// List of candidate names
    #[schema(example = json!(["Alice", "Bob", "Charlie"]))]
    pub candidates: Vec<String>,
    /// Number of seats to be filled in the election
    #[schema(example = 3)]
    pub num_seats: u32,
    /// Whether the order of elected candidates matters (true) or not (false)
    pub ordered_seats: bool,
    /// Election start time (ISO 8601 format)
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Election end time (ISO 8601 format)
    pub end_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ElectionResponse {
    /// Unique identifier for the election
    pub uuid: String,
    /// Admin UUID used for authentication and authorization
    pub admin_uuid: String,
    /// Title of the election
    pub title: String,
    /// Optional description providing additional details about the election
    pub description: Option<String>,
    /// List of candidate names
    pub candidates: Vec<String>,
    /// Number of seats to be filled in the election
    pub num_seats: u32,
    /// Whether the order of elected candidates matters
    pub ordered_seats: bool,
    /// Election start time (ISO 8601 format)
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Election end time (ISO 8601 format)
    pub end_time: chrono::DateTime<chrono::Utc>,
    /// Whether the election is locked (has redeemed tokens and cannot be modified)
    pub is_locked: bool,
}
