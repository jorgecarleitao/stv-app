use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateElectionRequest {
    pub title: String,
    pub description: Option<String>,
    pub candidates: Vec<String>,
    pub num_seats: u32,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ElectionResponse {
    pub uuid: String,
    pub admin_uuid: String,
    pub title: String,
    pub description: Option<String>,
    pub candidates: Vec<String>,
    pub num_seats: u32,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
}
