//! HTTP server that lets Home Assistant (or anything else on the LAN)
//! drive the dashboard remotely.
//!
//! Routes:
//! - `POST /widget/<name>` — switch to the named widget
//! - `POST /blank/on`  — blank the screen
//! - `POST /blank/off` — unblank the screen
//! - `POST /blank/toggle` — toggle the blank state
//!
//! When `token` is configured every request must carry
//! `Authorization: Bearer <token>`; otherwise auth is skipped.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use log::{error, info, warn};

use crate::config::RemoteControlConfig;

#[derive(Clone)]
struct AppState {
    name_to_id: Arc<HashMap<String, i32>>,
    token: Option<Arc<String>>,
    dashboard: slint::Weak<crate::Dashboard>,
}

pub fn spawn(
    config: RemoteControlConfig,
    name_to_id: HashMap<String, i32>,
    dashboard: slint::Weak<crate::Dashboard>,
) {
    let state = AppState {
        name_to_id: Arc::new(name_to_id),
        token: config.token.map(Arc::new),
        dashboard,
    };
    let listen = config.listen;

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            let app = Router::new()
                .route("/widget/:name", post(switch_widget))
                .route("/blank/:action", post(set_blank))
                .with_state(state);

            info!("Remote control: listening on http://{listen}");
            let listener = match tokio::net::TcpListener::bind(listen).await {
                Ok(l) => l,
                Err(e) => {
                    error!("Remote control: failed to bind {listen}: {e}");
                    return;
                }
            };
            if let Err(e) = axum::serve(listener, app).await {
                error!("Remote control: server error: {e}");
            }
        });
    });
}

/// Returns `Err(401)` if a token is configured and the request's
/// `Authorization` header does not match `Bearer <token>`.
fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let Some(expected) = state.token.as_deref() else {
        return Ok(());
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if provided == Some(expected.as_str()) {
        Ok(())
    } else {
        warn!("Remote control: rejected request with missing/bad bearer token");
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn switch_widget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<&'static str, StatusCode> {
    check_auth(&state, &headers)?;
    let Some(&id) = state.name_to_id.get(&name) else {
        warn!("Remote control: unknown widget '{name}'");
        return Err(StatusCode::NOT_FOUND);
    };
    info!("Remote control: switching to widget '{name}' (id={id})");
    let handle = state.dashboard.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(dashboard) = handle.upgrade() {
            dashboard.invoke_activate_widget(id);
        }
    });
    Ok("ok")
}

async fn set_blank(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(action): Path<String>,
) -> Result<&'static str, StatusCode> {
    check_auth(&state, &headers)?;
    let op = match action.as_str() {
        "on" => BlankOp::On,
        "off" => BlankOp::Off,
        "toggle" => BlankOp::Toggle,
        _ => {
            warn!("Remote control: unknown blank action '{action}'");
            return Err(StatusCode::NOT_FOUND);
        }
    };
    info!("Remote control: blank {action}");
    let handle = state.dashboard.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(dashboard) = handle.upgrade() {
            let value = match op {
                BlankOp::On => true,
                BlankOp::Off => false,
                BlankOp::Toggle => !dashboard.get_blanked(),
            };
            dashboard.set_blanked(value);
        }
    });
    Ok("ok")
}

enum BlankOp {
    On,
    Off,
    Toggle,
}
