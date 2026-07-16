use slint::ComponentHandle;

use crate::config::DailyVerseConfig;
use crate::widget::Widget;
use log::{error, info, warn};

const WIDGET_ID: i32 = 3;
const BIBLEGATEWAY_VOTD_URL: &str = "https://www.biblegateway.com/votd/get/";
const DEFAULT_VERSIONS: &[&str] = &["NGU-DE", "SCH2000"];
/// Retry interval on fetch failure (1 hour).
const RETRY_SECS: u64 = 3600;

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum VotdResponse {
    Success { votd: VotdData },
    Error { error: VotdError },
}

#[derive(Debug, serde::Deserialize)]
struct VotdData {
    text: String,
    display_ref: String,
    version: String,
}

#[derive(Debug, serde::Deserialize)]
struct VotdError {
    code: String,
    message: String,
}

/// Fetch the verse of the day for the given BibleGateway version ID.
async fn fetch_verse(client: &reqwest::Client, version: &str) -> Option<VotdData> {
    let url = format!("{BIBLEGATEWAY_VOTD_URL}?format=json&version={version}");
    info!("Fetching daily verse from {url}");
    match try_fetch(client, &url).await {
        Ok(votd) => {
            info!("Got daily verse: {} ({})", votd.display_ref, votd.version);
            Some(votd)
        }
        Err(e) => {
            error!("Daily verse fetch failed (version={version}): {e}");
            None
        }
    }
}

async fn try_fetch(client: &reqwest::Client, url: &str) -> Result<VotdData, String> {
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("reading body (status {status}): {e}"))?;
    let body_preview = || body.chars().take(128).collect::<String>();
    if !status.is_success() {
        return Err(format!("status {status}: {}", body_preview()));
    }
    match serde_json::from_str::<VotdResponse>(&body)
        .map_err(|e| format!("JSON parse error: {e}; body: {}", body_preview()))?
    {
        VotdResponse::Success { votd } => Ok(votd),
        VotdResponse::Error { error: e } => {
            Err(format!("API error code={}: {}", e.code, e.message))
        }
    }
}

/// Decode HTML entities in `s` and return the clean string.
fn decode_html(s: &str) -> String {
    htmlize::unescape(s).into_owned()
}

/// Seconds until midnight (local time), minimum 60 seconds.
fn secs_until_midnight() -> u64 {
    use chrono::Local;
    let now = Local::now();
    let tomorrow = (now + chrono::Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight time");
    let tomorrow = tomorrow
        .and_local_timezone(Local)
        .single()
        .expect("unambiguous local midnight");
    let delta = (tomorrow - now).num_seconds();
    (delta as u64).max(60)
}

/// Push the verse data to the UI.
fn push_to_ui(
    ui_handle: &slint::Weak<crate::Dashboard>,
    text: slint::SharedString,
    reference: slint::SharedString,
    version: slint::SharedString,
) {
    let handle = ui_handle.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(dashboard) = handle.upgrade() {
            dashboard.set_verse_text(text);
            dashboard.set_verse_reference(reference);
            dashboard.set_verse_version(version);
        }
    });
}

/// Daily verse widget — fetches the BibleGateway verse of the day.
///
/// Spawns a background thread that polls once per day (at midnight).
pub struct DailyVerseWidget {
    config: Option<DailyVerseConfig>,
}

impl DailyVerseWidget {
    pub fn new(config: DailyVerseConfig) -> Self {
        Self {
            config: Some(config),
        }
    }
}

impl Widget for DailyVerseWidget {
    fn id(&self) -> i32 {
        WIDGET_ID
    }

    fn name(&self) -> &'static str {
        "dailyverse"
    }

    fn init(&mut self, dashboard: &crate::Dashboard) {
        let ui_handle = dashboard.as_weak();
        let config = self
            .config
            .take()
            .expect("DailyVerseWidget::init called twice");
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_daily_verse_client(config, ui_handle));
        });
    }
}

async fn run_daily_verse_client(
    config: DailyVerseConfig,
    ui_handle: slint::Weak<crate::Dashboard>,
) {
    let versions: Vec<String> = config
        .versions
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_VERSIONS.iter().map(|s| s.to_string()).collect());
    info!("Starting daily verse client (versions={versions:?})");

    let client = reqwest::Client::new();

    loop {
        let sleep_secs = match fetch_first(&client, &versions).await {
            Some(votd) => {
                push_to_ui(
                    &ui_handle,
                    decode_html(&votd.text).into(),
                    decode_html(&votd.display_ref).into(),
                    decode_html(&votd.version).into(),
                );
                let s = secs_until_midnight();
                info!("Daily verse: sleeping {s}s until midnight");
                s
            }
            None => {
                warn!(
                    "Daily verse: all {} version(s) failed, retrying in {RETRY_SECS}s",
                    versions.len()
                );
                RETRY_SECS
            }
        };
        tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
    }
}

async fn fetch_first(client: &reqwest::Client, versions: &[String]) -> Option<VotdData> {
    for v in versions {
        if let Some(votd) = fetch_verse(client, v).await {
            return Some(votd);
        }
    }
    None
}
