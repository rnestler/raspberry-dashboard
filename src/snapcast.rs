use std::net::SocketAddr;

use snapcast_control::{
    SnapcastConnection, State,
    stream::{Stream, StreamStatus},
};
use std::sync::Arc;

use slint::ComponentHandle;

use crate::config::SnapcastConfig;
use crate::widget::Widget;

const WIDGET_INDEX: i32 = 1;

/// Snapcast now-playing widget.
///
/// Spawns a background thread that connects to a Snapcast server and
/// auto-switches the dashboard to this widget when a stream is playing.
/// This is a special case: the background thread directly manipulates
/// `current_widget` via `Weak<Dashboard>`.
pub struct SnapcastWidget {
    config: SnapcastConfig,
}

impl SnapcastWidget {
    pub fn new(config: SnapcastConfig) -> Self {
        Self { config }
    }
}

impl Widget for SnapcastWidget {
    fn index(&self) -> i32 {
        WIDGET_INDEX
    }

    fn init(&mut self, dashboard: &crate::Dashboard, fallback_widget: i32) {
        let ui_handle = dashboard.as_weak();
        let addr = self.config.host;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    run_snapcast_client(addr, ui_handle.clone(), fallback_widget).await;
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            });
        });
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingInfo {
    pub title: Option<String>,
    pub artist: Option<Vec<String>>,
    pub album: Option<String>,
    pub art_url: Option<String>,
}

fn get_now_playing_info(stream: Option<&Stream>) -> Option<NowPlayingInfo> {
    let stream = stream?;
    if stream.status != StreamStatus::Playing {
        return None;
    }
    if let Some(metadata) = &stream.properties.as_ref()?.metadata
        && let Ok(value) = serde_json::to_value(metadata)
        && let Ok(info) = serde_json::from_value::<NowPlayingInfo>(value)
    {
        return Some(info);
    }
    None
}

fn extract_now_playing(state: &Arc<State>) -> Option<NowPlayingInfo> {
    for entry in state.streams.iter() {
        if let Some(info) = get_now_playing_info(entry.value().as_ref()) {
            return Some(info);
        }
    }
    None
}

async fn fetch_art_bytes(url: &str) -> Option<Vec<u8>> {
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?;
    Some(bytes.to_vec())
}

async fn push_to_ui(
    ui_handle: &slint::Weak<crate::Dashboard>,
    info: Option<&NowPlayingInfo>,
    status: &str,
    fallback_widget: i32,
) {
    let handle = ui_handle.clone();
    let info = info.cloned();
    let status = status.to_string();
    let art_bytes = match info.as_ref().and_then(|i| i.art_url.as_deref()) {
        Some(url) => fetch_art_bytes(url).await,
        None => None,
    };
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(dashboard) = handle.upgrade() {
            if let Some(info) = info {
                dashboard.set_track_title(info.title.unwrap_or_default().into());
                dashboard.set_track_artist(
                    info.artist
                        .map(|a| a.join(", "))
                        .unwrap_or("Unknown Artist".into())
                        .into(),
                );
                dashboard.set_track_album(info.album.unwrap_or("Unknown Album".into()).into());
                let art_image = art_bytes
                    .as_deref()
                    .and_then(|b| slint::Image::load_from_svg_data(b).ok())
                    .unwrap_or_default();
                dashboard.set_art_image(art_image);
                dashboard.set_current_widget(1);
            } else {
                dashboard.set_current_widget(fallback_widget);
            }
            dashboard.set_connection_status(status.into());
        }
    });
}

fn set_connection_status(ui_handle: &slint::Weak<crate::Dashboard>, status: &str) {
    let handle = ui_handle.clone();
    let status = status.to_string();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(dashboard) = handle.upgrade() {
            dashboard.set_connection_status(status.into());
        }
    });
}

pub async fn run_snapcast_client(
    addr: SocketAddr,
    ui_handle: slint::Weak<crate::Dashboard>,
    fallback_widget: i32,
) {
    set_connection_status(&ui_handle, "Connecting...");

    let mut client = match SnapcastConnection::open(addr).await {
        Ok(client) => client,
        Err(e) => {
            set_connection_status(&ui_handle, &format!("Connection error: {e}"));
            return;
        }
    };

    set_connection_status(&ui_handle, "connected");

    if let Err(e) = client.server_get_status().await {
        set_connection_status(&ui_handle, &format!("Error: {e}"));
        return;
    }

    // Receive the initial status response and update UI
    if let Some(messages) = client.recv().await {
        for msg in &messages {
            if let Err(e) = msg {
                log::error!("Snapcast message error: {e}");
            }
        }
        let info = extract_now_playing(&client.state);
        push_to_ui(&ui_handle, info.as_ref(), "connected", fallback_widget).await;
    }

    // Keep receiving notifications and updating state
    while let Some(messages) = client.recv().await {
        for msg in &messages {
            if let Err(e) = msg {
                log::error!("Snapcast message error: {e}");
            }
        }
        let info = extract_now_playing(&client.state);
        push_to_ui(&ui_handle, info.as_ref(), "connected", fallback_widget).await;
    }

    set_connection_status(&ui_handle, "Disconnected");
}
