use slint::ComponentHandle;

use crate::config::WeatherConfig;
use crate::widget::Widget;
use log::{error, info};

const WIDGET_ID: i32 = 5;
const DEFAULT_POLL_SECS: u64 = 300;
const DEFAULT_FORECAST_DAYS: usize = 5;

/// Weather widget backed by a Home Assistant weather entity.
///
/// Spawns a background thread that polls current conditions and
/// forecasts from Home Assistant at a configurable interval.
pub struct WeatherWidget {
    config: Option<WeatherConfig>,
    token: String,
}

impl WeatherWidget {
    pub fn new(config: WeatherConfig, token: String) -> Self {
        Self {
            config: Some(config),
            token,
        }
    }
}

impl Widget for WeatherWidget {
    fn id(&self) -> i32 {
        WIDGET_ID
    }

    fn init(&mut self, dashboard: &crate::Dashboard) {
        let ui_handle = dashboard.as_weak();
        let config = self
            .config
            .take()
            .expect("WeatherWidget::init called twice");
        let token = self.token.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_weather_client(config, token, ui_handle));
        });
    }
}

// ── HA API response types ───────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct StateResponse {
    state: String,
    attributes: WeatherAttributes,
}

#[derive(Debug, serde::Deserialize)]
struct WeatherAttributes {
    temperature: Option<f64>,
    humidity: Option<f64>,
    wind_speed: Option<f64>,
    #[serde(default)]
    temperature_unit: String,
    #[serde(default)]
    wind_speed_unit: String,
}

#[derive(Debug, serde::Deserialize)]
struct ForecastEntry {
    datetime: String,
    condition: Option<String>,
    temperature: Option<f64>,
    templow: Option<f64>,
}

// ── Condition → Unicode symbol mapping ──────────────────────────────

fn condition_symbol(condition: &str) -> &'static str {
    match condition {
        "sunny" => "☀",
        "clear-night" => "🌙",
        "partlycloudy" => "⛅",
        "cloudy" => "☁",
        "rainy" => "🌧",
        "pouring" => "🌧",
        "snowy" => "❄",
        "snowy-rainy" => "🌨",
        "fog" => "🌫",
        "hail" => "🌨",
        "lightning" => "🌩",
        "lightning-rainy" => "⛈",
        "windy" | "windy-variant" => "💨",
        "exceptional" => "⚠",
        _ => "?",
    }
}

// ── Fetch helpers ───────────────────────────────────────────────────

async fn fetch_current(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    entity_id: &str,
) -> Option<StateResponse> {
    let request_url = format!("{url}/api/states/{entity_id}");
    info!("Weather: fetching {request_url}");
    let response = match client.get(&request_url).bearer_auth(token).send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Weather: request error: {e}");
            return None;
        }
    };
    info!("Weather: current status={}", response.status());
    match response.json::<StateResponse>().await {
        Ok(resp) => Some(resp),
        Err(e) => {
            error!("Weather: JSON parse error: {e}");
            None
        }
    }
}

async fn fetch_forecast(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    entity_id: &str,
    forecast_type: &str,
) -> Vec<ForecastEntry> {
    let request_url = format!("{url}/api/services/weather/get_forecasts?return_response");
    info!("Weather: fetching forecast ({forecast_type}) from {request_url}");

    let body = serde_json::json!({
        "entity_id": entity_id,
        "type": forecast_type,
    });

    let response = match client
        .post(&request_url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            error!("Weather: forecast request error: {e}");
            return Vec::new();
        }
    };
    info!("Weather: forecast status={}", response.status());

    // The response wraps forecasts under `service_response` →
    // `<entity_id>` → `forecast`.
    let json: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(e) => {
            error!("Weather: forecast JSON parse error: {e}");
            return Vec::new();
        }
    };

    // Navigate: service_response.<entity_id>.forecast
    json.get("service_response")
        .and_then(|sr| sr.get(entity_id))
        .and_then(|ent| ent.get("forecast"))
        .and_then(|f| serde_json::from_value::<Vec<ForecastEntry>>(f.clone()).ok())
        .unwrap_or_default()
}

// ── Background polling loop ─────────────────────────────────────────

async fn run_weather_client(
    config: WeatherConfig,
    token: String,
    ui_handle: slint::Weak<crate::Dashboard>,
) {
    info!("Starting weather client for {}", config.url);
    let client = reqwest::Client::new();
    let poll_interval =
        std::time::Duration::from_secs(config.poll_interval_secs.unwrap_or(DEFAULT_POLL_SECS));
    let forecast_days = config.forecast_days.unwrap_or(DEFAULT_FORECAST_DAYS);
    let forecast_type = config
        .forecast_type
        .as_deref()
        .unwrap_or("daily")
        .to_owned();

    loop {
        // Fetch current conditions and forecast concurrently.
        let (current, forecast_entries) = tokio::join!(
            fetch_current(&client, &config.url, &token, &config.entity_id),
            fetch_forecast(
                &client,
                &config.url,
                &token,
                &config.entity_id,
                &forecast_type,
            ),
        );

        let handle = ui_handle.clone();
        let fc_days = forecast_days;
        let _ = slint::invoke_from_event_loop(move || {
            let Some(dashboard) = handle.upgrade() else {
                return;
            };

            if let Some(state) = current {
                let symbol = condition_symbol(&state.state);
                let temp = state
                    .attributes
                    .temperature
                    .map(|t| format!("{t:.0}"))
                    .unwrap_or_default();
                let temp_unit = state.attributes.temperature_unit.clone();
                let humidity = state
                    .attributes
                    .humidity
                    .map(|h| format!("{h:.0}%"))
                    .unwrap_or_default();
                let wind = state
                    .attributes
                    .wind_speed
                    .map(|w| format!("{w:.0} {}", state.attributes.wind_speed_unit))
                    .unwrap_or_default();

                dashboard.set_weather_condition_symbol(symbol.into());
                dashboard.set_weather_condition(state.state.into());
                dashboard.set_weather_temp(format!("{temp}{temp_unit}").into());
                dashboard.set_weather_humidity(humidity.into());
                dashboard.set_weather_wind(wind.into());
            }

            let forecasts: Vec<crate::ForecastEntry> = forecast_entries
                .into_iter()
                .take(fc_days)
                .map(|e| {
                    // Parse datetime to extract day name.
                    let day = parse_day(&e.datetime);
                    let cond = e.condition.as_deref().unwrap_or("unknown");
                    let symbol = condition_symbol(cond);
                    let high = e
                        .temperature
                        .map(|t| format!("{t:.0}°"))
                        .unwrap_or_default();
                    let low = e.templow.map(|t| format!("{t:.0}°")).unwrap_or_default();
                    crate::ForecastEntry {
                        day: day.into(),
                        condition_symbol: symbol.into(),
                        temp_high: high.into(),
                        temp_low: low.into(),
                    }
                })
                .collect();

            let model = std::rc::Rc::new(slint::VecModel::from(forecasts));
            dashboard.set_weather_forecasts(model.into());
        });

        tokio::time::sleep(poll_interval).await;
    }
}

/// Extract a short day name (e.g. "Mon") from an ISO 8601 datetime string.
fn parse_day(datetime: &str) -> String {
    // HA returns datetimes like "2024-03-15T12:00:00+01:00".
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(datetime) {
        return dt.format("%a").to_string();
    }
    // Fallback: try parsing just the date portion.
    if let Ok(date) = chrono::NaiveDate::parse_from_str(&datetime[..10], "%Y-%m-%d") {
        return date.format("%a").to_string();
    }
    datetime[..3.min(datetime.len())].to_string()
}
