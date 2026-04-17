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

const WIDGET_ID: i32 = 1;

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
    fn id(&self) -> i32 {
        WIDGET_ID
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
        log::info!("{info:?}");
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

fn load_image_from_bytes(data: &[u8]) -> Option<slint::Image> {
    // Try SVG first.
    if let Ok(img) = slint::Image::load_from_svg_data(data) {
        return Some(img);
    }
    // Fall back to raster (PNG/JPEG).
    let decoded = image::load_from_memory(data)
        .map_err(|e| log::error!("Album art decode error: {e}"))
        .ok()?;
    let rgba = decoded.to_rgba8();
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    );
    Some(slint::Image::from_rgba8(buffer))
}

async fn fetch_art_bytes(url: &str) -> Option<Vec<u8>> {
    log::info!("Fetching album art from {url}");
    let response = reqwest::get(url)
        .await
        .map_err(|e| log::error!("Album art fetch error: {e}"))
        .ok()?;
    log::info!("Album art response status={}", response.status());
    let bytes = response
        .bytes()
        .await
        .map_err(|e| log::error!("Album art read error: {e}"))
        .ok()?;
    log::info!("Album art: {} bytes", bytes.len());
    Some(bytes.to_vec())
}

/// Update UI properties and toggle the active flag.
///
/// When `info` is `Some` (stream playing): set track metadata and
/// pre-fetched album art, mark active, switch dashboard to this widget.
/// When `info` is `None` (no stream): mark inactive, invoke
/// `deactivate-widget` so the dashboard can switch away.
fn push_to_ui(
    ui_handle: &slint::Weak<crate::Dashboard>,
    info: Option<&NowPlayingInfo>,
    art_bytes: Option<&Vec<u8>>,
    status: &str,
    active: &Arc<AtomicBool>,
) {
    let handle = ui_handle.clone();
    let info = info.cloned();
    let status = status.to_string();
    let art_bytes = art_bytes.cloned();
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
                    .and_then(load_image_from_bytes)
                    .unwrap_or_default();
                dashboard.set_art_image(art_image);
                dashboard.invoke_activate_widget(WIDGET_ID);
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

    let mut art_cache: Option<(String, Vec<u8>)> = None;

    while let Some(messages) = client.recv().await {
        for msg in &messages {
            if let Err(e) = msg {
                log::error!("Snapcast message error: {e}");
            }
        }
        let info = extract_now_playing(&client.state);
        let art_url = info.as_ref().and_then(|i| i.art_url.as_deref());

        // Only re-fetch when the URL changes.
        let cached_album_art = match art_url {
            Some(url) => {
                let hit = art_cache
                    .as_ref()
                    .is_some_and(|(cached_url, _)| cached_url == url);
                if !hit {
                    art_cache = fetch_art_bytes(url)
                        .await
                        .map(|bytes| (url.to_string(), bytes));
                }
                art_cache.as_ref().map(|(_, bytes)| bytes)
            }
            None => {
                art_cache = None;
                None
            }
        };

        push_to_ui(
            &ui_handle,
            info.as_ref(),
            cached_album_art,
            "connected",
            &active,
        );
    }

    set_connection_status(&ui_handle, "Disconnected");
}
