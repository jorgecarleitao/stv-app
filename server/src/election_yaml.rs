use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionConfig {
    pub id: String,
    pub name: String,
    pub candidates: Vec<String>,
    pub seats: usize,
}

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
                let config: ElectionConfig = serde_yaml::from_str(&content).ok()?;
                Some((config.id.clone(), config))
            } else {
                None
            }
        })
        .collect();
    Ok(map)
}
