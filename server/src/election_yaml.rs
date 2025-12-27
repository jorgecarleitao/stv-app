use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ElectionConfig - what gets sent to the API (no sensitive info)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionConfig {
    pub id: String,
    pub name: String,
    pub candidates: Vec<String>,
    pub seats: usize,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ballots: Option<Vec<String>>, // populated only after election ends
}

/// ElectionConfigFile - what is stored in YAML (includes valid ballot UUIDs)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionConfigFile {
    pub id: String,
    pub name: String,
    pub candidates: Vec<String>,
    pub seats: usize,
    #[serde(default)]
    pub ballots: Vec<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}

impl ElectionConfigFile {
    /// Convert internal file config to public API config based on current time (consumes by value)
    pub fn into_public(self, now: DateTime<Utc>) -> ElectionConfig {
        ElectionConfig {
            id: self.id,
            name: self.name,
            candidates: self.candidates,
            seats: self.seats,
            start_time: self.start_time,
            end_time: self.end_time,
            ballots: is_public(self.end_time, now).then_some(self.ballots),
        }
    }
}

/// Derived visibility: election is public after `end_time`
fn is_public(end: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now >= end
}

/// Load a single election YAML file by ID
pub fn load_election(
    dir: &str,
    election_id: &str,
) -> Result<ElectionConfigFile, Box<dyn std::error::Error>> {
    let path = format!("{}/{}.yaml", dir, election_id);
    let content = std::fs::read_to_string(&path)?;
    let config: ElectionConfigFile = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Load all elections from directory
pub fn load_elections(
    dir: &str,
) -> Result<HashMap<String, ElectionConfig>, Box<dyn std::error::Error>> {
    let map: HashMap<String, ElectionConfig> = std::fs::read_dir(dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path
                .extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml")
            {
                let content = std::fs::read_to_string(&path).ok()?;
                let config: ElectionConfigFile = serde_yaml::from_str(&content).ok()?;
                let public = config.into_public(Utc::now());
                Some((public.id.clone(), public))
            } else {
                None
            }
        })
        .collect();
    Ok(map)
}
