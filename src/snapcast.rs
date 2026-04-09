use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use slint::ComponentHandle;
use snapcast_control::{
    SnapcastConnection, State,
    stream::{Stream, StreamStatus},
};

use crate::config::SnapcastConfig;
use crate::widget::Widget;

const WIDGET_INDEX: i32 = 1;

/// Snapcast now-playing widget.
///
/// Spawns a background thread that connects to a Snapcast server.
/// When a stream is playing the widget marks itself *active* and switches
/// the dashboard to itself.  When playback stops it marks itself *inactive*
/// and invokes the `deactivate-widget` Slint callback so the dashboard can
/// switch to the next active widget.
pub struct SnapcastWidget {
    config: SnapcastConfig,
    active: Arc<AtomicBool>,
}

impl SnapcastWidget {
    pub fn new(config: SnapcastConfig) -> Self {
        Self {
            config,
            active: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Widget for SnapcastWidget {
    fn index(&self) -> i32 {
        WIDGET_INDEX
    }

    fn init(&mut self, dashboard: &crate::Dashboard) {
        let ui_handle = dashboard.as_weak();
        let addr = self.config.host;
        let active = Arc::clone(&self.active);
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    run_snapcast_client(addr, ui_handle.clone(), Arc::clone(&active)).await;
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            });
        });
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
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

/// Update UI properties and toggle the active flag.
///
/// When `info` is `Some` (stream playing): set track metadata, mark active,
/// switch dashboard to this widget.
/// When `info` is `None` (no stream): mark inactive, invoke
/// `deactivate-widget` so the dashboard can switch away.
async fn push_to_ui(
    ui_handle: &slint::Weak<crate::Dashboard>,
    info: Option<&NowPlayingInfo>,
    status: &str,
    active: &Arc<AtomicBool>,
) {
    let handle = ui_handle.clone();
    let info = info.cloned();
    let status = status.to_string();
    let art_bytes = match info.as_ref().and_then(|i| i.art_url.as_deref()) {
        Some(url) => fetch_art_bytes(url).await,
        None => None,
    };
    let is_playing = info.is_some();
    active.store(is_playing, Ordering::Relaxed);
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
                dashboard.set_current_widget(WIDGET_INDEX);
            } else {
                dashboard.invoke_deactivate_widget();
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

async fn run_snapcast_client(
    addr: SocketAddr,
    ui_handle: slint::Weak<crate::Dashboard>,
    active: Arc<AtomicBool>,
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
        push_to_ui(&ui_handle, info.as_ref(), "connected", &active).await;
    }

    // Keep receiving notifications and updating state
    while let Some(messages) = client.recv().await {
        for msg in &messages {
            if let Err(e) = msg {
                log::error!("Snapcast message error: {e}");
            }
        }
        let info = extract_now_playing(&client.state);
        push_to_ui(&ui_handle, info.as_ref(), "connected", &active).await;
    }

    set_connection_status(&ui_handle, "Disconnected");
}
