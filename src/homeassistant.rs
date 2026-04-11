use slint::ComponentHandle;

use crate::config::HomeAssistantConfig;
use crate::widget::Widget;
use log::{error, info, warn};

const WIDGET_ID: i32 = 0;

/// Home Assistant sensor widget.
///
/// Spawns a background thread that polls sensor states from a
/// Home Assistant instance at a configurable interval.
pub struct HomeAssistantWidget {
    config: Option<HomeAssistantConfig>,
}

impl HomeAssistantWidget {
    pub fn new(config: HomeAssistantConfig) -> Self {
        Self {
            config: Some(config),
        }
    }
}

impl Widget for HomeAssistantWidget {
    fn id(&self) -> i32 {
        WIDGET_ID
    }

    fn init(&mut self, dashboard: &crate::Dashboard) {
        let ui_handle = dashboard.as_weak();
        let config = self
            .config
            .take()
            .expect("HomeAssistantWidget::init called twice");
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_homeassistant_client(config, ui_handle));
        });
    }
}

#[derive(Debug, serde::Deserialize)]
struct StateResponse {
    state: String,
    attributes: StateAttributes,
}

#[derive(Debug, serde::Deserialize)]
struct StateAttributes {
    unit_of_measurement: Option<String>,
}

async fn fetch_sensor(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    entity_id: &str,
) -> Option<(String, String)> {
    let request_url = format!("{url}/api/states/{entity_id}");
    info!("Fetching {request_url}");
    let response = match client.get(&request_url).bearer_auth(token).send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Request error for {entity_id}: {e}");
            return None;
        }
    };
    info!("{entity_id} status={}", response.status());
    let resp = match response.json::<StateResponse>().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("JSON parse error for {entity_id}: {e}");
            return None;
        }
    };
    info!(
        "{entity_id} = {} {:?}",
        resp.state, resp.attributes.unit_of_measurement
    );
    let unit = resp.attributes.unit_of_measurement.unwrap_or_default();
    Some((resp.state, unit))
}

/// Build a `SensorData` value for a gauge sensor.
///
/// Logs a warning and returns `None` if the sensor config is incomplete or
/// the value cannot be parsed as a number.
fn build_gauge_data(
    sensor: &crate::config::SensorConfig,
    label: &str,
    value: &str,
    unit: &str,
) -> Option<crate::SensorData> {
    let min_value = match sensor.min {
        Some(v) => v,
        None => {
            warn!(
                "Gauge sensor '{}' missing 'min' – falling back to plain card",
                label
            );
            return None;
        }
    };
    let max_value = match sensor.max {
        Some(v) => v,
        None => {
            warn!(
                "Gauge sensor '{}' missing 'max' – falling back to plain card",
                label
            );
            return None;
        }
    };
    if max_value <= min_value {
        warn!(
            "Gauge sensor '{}': max ({}) must be greater than min ({}) – falling back to plain card",
            label, max_value, min_value
        );
        return None;
    }

    let thresholds = sensor.thresholds.as_deref().unwrap_or(&[]);
    if thresholds.len() < 3 {
        warn!(
            "Gauge sensor '{}': 'thresholds' must contain exactly 3 values (got {}) – falling back to plain card",
            label,
            thresholds.len()
        );
        return None;
    }
    let (t1, t2, t3) = (thresholds[0], thresholds[1], thresholds[2]);
    if !(t1 <= t2 && t2 <= t3) {
        warn!(
            "Gauge sensor '{}': thresholds must be ascending ({}, {}, {}) – falling back to plain card",
            label, t1, t2, t3
        );
        return None;
    }

    let current_value = match value.parse::<f32>() {
        Ok(v) => v,
        Err(_) => {
            // Sensor state is not numeric (e.g. "unavailable") – show plain card.
            warn!(
                "Gauge sensor '{}': state '{}' is not numeric – falling back to plain card",
                label, value
            );
            return None;
        }
    };

    Some(crate::SensorData {
        label: label.into(),
        value: value.into(),
        unit: unit.into(),
        is_gauge: true,
        min_value,
        max_value,
        current_value,
        threshold1: t1,
        threshold2: t2,
        threshold3: t3,
    })
}

async fn run_homeassistant_client(
    config: HomeAssistantConfig,
    ui_handle: slint::Weak<crate::Dashboard>,
) {
    info!(
        "Starting HA client for {} with {} sensors",
        config.url,
        config.sensors.len()
    );
    let client = reqwest::Client::new();
    let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs.unwrap_or(30));
    let labels: Vec<String> = config.sensors.iter().map(|s| s.label.clone()).collect();

    loop {
        let mut readings: Vec<crate::SensorData> = Vec::new();

        for (i, sensor) in config.sensors.iter().enumerate() {
            let (value, unit) =
                fetch_sensor(&client, &config.url, &config.token, &sensor.entity_id)
                    .await
                    .unwrap_or_else(|| ("unavailable".into(), String::new()));

            let label = &labels[i];
            let is_gauge = sensor.sensor_type.as_deref() == Some("gauge");

            let data = if is_gauge {
                build_gauge_data(sensor, label, &value, &unit).unwrap_or_else(|| {
                    // Fall back to a plain card if gauge config is invalid.
                    crate::SensorData {
                        label: label.as_str().into(),
                        value: value.as_str().into(),
                        unit: unit.as_str().into(),
                        is_gauge: false,
                        min_value: 0.0,
                        max_value: 0.0,
                        current_value: 0.0,
                        threshold1: 0.0,
                        threshold2: 0.0,
                        threshold3: 0.0,
                    }
                })
            } else {
                crate::SensorData {
                    label: label.as_str().into(),
                    value: value.as_str().into(),
                    unit: unit.as_str().into(),
                    is_gauge: false,
                    min_value: 0.0,
                    max_value: 0.0,
                    current_value: 0.0,
                    threshold1: 0.0,
                    threshold2: 0.0,
                    threshold3: 0.0,
                }
            };
            readings.push(data);
        }

        let handle = ui_handle.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(dashboard) = handle.upgrade() {
                let model = std::rc::Rc::new(slint::VecModel::from(readings));
                dashboard.set_sensors(model.into());
            }
        });

        tokio::time::sleep(poll_interval).await;
    }
}
