use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ElectionConfig - what gets sent to the API (no sensitive info)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionConfig {
    pub id: String,
    pub name: String,
    pub candidates: Vec<String>,
    pub seats: usize,
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
}

impl From<ElectionConfigFile> for ElectionConfig {
    fn from(config: ElectionConfigFile) -> Self {
        ElectionConfig {
            id: config.id,
            name: config.name,
            candidates: config.candidates,
            seats: config.seats,
        }
    }
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
                Some((config.id.clone(), config.into()))
            } else {
                None
            }
        })
        .collect();
    Ok(map)
}
