use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionConfig {
    pub id: String,
    pub name: String,
    pub candidates: Vec<String>,
    pub seats: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallotSubmission {
    pub ranked_choices: Vec<Option<usize>>, // indices into candidates list
}

pub fn load_elections(
    elections_dir: &str,
) -> Result<HashMap<String, ElectionConfig>, Box<dyn std::error::Error>> {
    let map: HashMap<String, ElectionConfig> = std::fs::read_dir(elections_dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path
                .extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml")
            {
                let content = std::fs::read_to_string(&path).ok()?;
                let config: ElectionConfig = serde_yaml::from_str(&content).ok()?;
                Some((config.id.clone(), config))
            } else {
                None
            }
        })
        .collect();
    Ok(map)
}
