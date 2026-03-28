use crate::config::HomeAssistantConfig;
use log::{error, info};

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

pub async fn run_homeassistant_client(
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
            readings.push(crate::SensorData {
                label: labels[i].clone().into(),
                value: value.into(),
                unit: unit.into(),
            });
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
