use serde::Deserialize;
use std::net::SocketAddr;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub snapcast: Option<SnapcastConfig>,
    pub homeassistant: Option<HomeAssistantConfig>,
    pub daily_verse: Option<DailyVerseConfig>,
    pub quotes: Option<QuotesConfig>,
    pub weather: Option<WeatherConfig>,
    pub remote_control: Option<RemoteControlConfig>,
    /// Automatically advance to the next enabled widget every N seconds.
    pub widget_cycle_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteControlConfig {
    /// Address to bind the HTTP server, e.g. "0.0.0.0:8765".
    pub listen: SocketAddr,
}

/// Read the remote-control shared bearer token from the
/// `DASHBOARD_REMOTE_TOKEN` environment variable.  Required to enable
/// the remote-control HTTP server; without it the `[remote_control]`
/// section is ignored.  Every request must include
/// `Authorization: Bearer <token>`.
pub fn remote_control_token() -> Option<String> {
    std::env::var("DASHBOARD_REMOTE_TOKEN").ok()
}

#[derive(Debug, Deserialize)]
pub struct SnapcastConfig {
    pub host: SocketAddr,
}

#[derive(Debug, Deserialize)]
pub struct QuotesConfig {
    pub items: Vec<QuoteItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuoteItem {
    pub text: String,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DailyVerseConfig {
    /// Ordered list of BibleGateway version IDs. The widget tries each in
    /// turn and uses the first that returns a verse — useful as a fallback
    /// when a preferred translation doesn't cover every book.
    pub versions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct HomeAssistantConfig {
    pub url: String,
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

/// Read the Home Assistant long-lived access token from the
/// `HOMEASSISTANT_TOKEN` environment variable.
pub fn homeassistant_token() -> Option<String> {
    std::env::var("HOMEASSISTANT_TOKEN").ok()
}

#[derive(Debug, Deserialize)]
pub struct WeatherConfig {
    pub url: String,
    pub entity_id: String,
    pub poll_interval_secs: Option<u64>,
    /// Number of forecast days to display (default: 5).
    pub forecast_days: Option<usize>,
    /// Forecast type: "daily" (default), "hourly", or "twice_daily".
    pub forecast_type: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_config() {
        let toml = r#"
            widget_cycle_secs = 30

            [snapcast]
            host = "127.0.0.1:1704"

            [homeassistant]
            url = "http://homeassistant.local:8123"
            poll_interval_secs = 60
            sensors = [{ entity_id = "sensor.temp", label = "Temp" }]

            [daily_verse]
            versions = ["NGU-DE"]

            [quotes]
            items = [{ text = "Hello", source = "World" }]

            [weather]
            url = "http://ha:8123"
            entity_id = "weather.home"
            forecast_days = 3
            forecast_type = "hourly"

            [remote_control]
            listen = "0.0.0.0:8765"
        "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.widget_cycle_secs, Some(30));
        assert!(cfg.snapcast.is_some());
        assert_eq!(cfg.snapcast.unwrap().host.to_string(), "127.0.0.1:1704");
        assert!(cfg.homeassistant.is_some());
        assert!(cfg.daily_verse.is_some());
        assert!(cfg.quotes.is_some());
        assert!(cfg.weather.is_some());
        assert_eq!(cfg.weather.unwrap().forecast_days, Some(3));
        assert!(cfg.remote_control.is_some());
    }

    #[test]
    fn deserialize_minimal_config() {
        let toml = r#"
            [quotes]
            items = [{ text = "Minimal" }]
        "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.quotes.is_some());
        assert!(cfg.snapcast.is_none());
        assert!(cfg.homeassistant.is_none());
        assert!(cfg.weather.is_none());
        assert!(cfg.daily_verse.is_none());
    }

    #[test]
    fn deserialize_sensor_with_gauge() {
        let toml = r#"
            entity_id = "sensor.battery"
            label = "Battery"
            sensor_type = "gauge"
            min = 0.0
            max = 100.0
            thresholds = [20.0, 50.0, 80.0]
        "#;
        let sensor: SensorConfig = toml::from_str(toml).unwrap();
        assert_eq!(sensor.entity_id, "sensor.battery");
        assert_eq!(sensor.sensor_type, Some("gauge".to_string()));
        assert_eq!(sensor.min, Some(0.0));
        assert_eq!(sensor.max, Some(100.0));
        assert_eq!(sensor.thresholds, Some(vec![20.0, 50.0, 80.0]));
    }

    #[test]
    fn default_config_is_empty() {
        let cfg = Config::default();
        assert!(cfg.snapcast.is_none());
        assert!(cfg.homeassistant.is_none());
        assert!(cfg.daily_verse.is_none());
        assert!(cfg.quotes.is_none());
        assert!(cfg.weather.is_none());
        assert!(cfg.remote_control.is_none());
        assert!(cfg.widget_cycle_secs.is_none());
    }
}
