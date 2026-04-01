use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub homeassistant: Option<HomeAssistantConfig>,
    pub daily_verse: Option<DailyVerseConfig>,
    /// Automatically advance to the next enabled widget every N seconds.
    pub widget_cycle_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct DailyVerseConfig {
    /// BibleGateway version ID (e.g. "NGU-DE", "LUTH1912"). Defaults to "NGU-DE".
    pub version: Option<String>,
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
    /// Display type: "gauge" or omit for the plain card.
    pub sensor_type: Option<String>,
    /// Gauge: minimum value of the range.
    pub min: Option<f32>,
    /// Gauge: maximum value of the range.
    pub max: Option<f32>,
    /// Gauge: exactly three ascending threshold values that define the
    /// boundaries between the blue/green, green/orange, and orange/red zones.
    pub thresholds: Option<Vec<f32>>,
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
