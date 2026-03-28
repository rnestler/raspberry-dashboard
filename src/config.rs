use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub homeassistant: Option<HomeAssistantConfig>,
}

#[derive(Debug, Deserialize)]
pub struct HomeAssistantConfig {
    pub url: String,
    pub token: String,
    pub poll_interval_secs: Option<u64>,
    pub sensors: Vec<SensorConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SensorConfig {
    pub entity_id: String,
    pub label: String,
}

pub fn load_config() -> Config {
    let path = std::env::var("DASHBOARD_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let path = Path::new(&path);
    if !path.exists() {
        log::info!("Config: no config file found at {}", path.display());
        return Config::default();
    }
    log::info!("Config: loading from {}", path.display());
    let contents = std::fs::read_to_string(path).expect("failed to read config file");
    toml::from_str(&contents).expect("failed to parse config file")
}
