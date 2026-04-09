use slint::ComponentHandle;

use crate::config::DailyVerseConfig;
use crate::widget::Widget;
use log::{error, info, warn};

const WIDGET_INDEX: i32 = 3;
const BIBLEGATEWAY_VOTD_URL: &str = "https://www.biblegateway.com/votd/get/";
const DEFAULT_VERSION: &str = "NGU-DE";
/// Retry interval on fetch failure (1 hour).
const RETRY_SECS: u64 = 3600;

#[derive(Debug, serde::Deserialize)]
struct VotdResponse {
    votd: VotdData,
}

#[derive(Debug, serde::Deserialize)]
struct VotdData {
    text: String,
    display_ref: String,
    version: String,
}

/// Fetch the verse of the day for the given BibleGateway version ID.
async fn fetch_verse(client: &reqwest::Client, version: &str) -> Option<VotdData> {
    let url = format!("{BIBLEGATEWAY_VOTD_URL}?format=json&version={version}");
    info!("Fetching daily verse from {url}");

    let response = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            error!("Daily verse fetch error: {e}");
            return None;
        }
    };

    if !response.status().is_success() {
        error!("Daily verse fetch returned status {}", response.status());
        return None;
    }

    match response.json::<VotdResponse>().await {
        Ok(r) => {
            info!(
                "Got daily verse: {} ({})",
                r.votd.display_ref, r.votd.version
            );
            Some(r.votd)
        }
        Err(e) => {
            error!("Daily verse JSON parse error: {e}");
            None
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
    fn index(&self) -> i32 {
        WIDGET_INDEX
    }

    fn init(&mut self, dashboard: &crate::Dashboard, _fallback_widget: i32) {
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
    let version = config
        .version
        .as_deref()
        .unwrap_or(DEFAULT_VERSION)
        .to_string();
    info!("Starting daily verse client (version={version})");

    let client = reqwest::Client::new();

    loop {
        match fetch_verse(&client, &version).await {
            Some(votd) => {
                let text = decode_html(&votd.text);
                let reference = decode_html(&votd.display_ref);
                let ver = decode_html(&votd.version);

                push_to_ui(&ui_handle, text.into(), reference.into(), ver.into());

                let sleep_secs = secs_until_midnight();
                info!("Daily verse: sleeping {sleep_secs}s until midnight");
                tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
            }
            None => {
                warn!("Daily verse: fetch failed, retrying in {RETRY_SECS}s");
                tokio::time::sleep(std::time::Duration::from_secs(RETRY_SECS)).await;
            }
        }
    }
}
