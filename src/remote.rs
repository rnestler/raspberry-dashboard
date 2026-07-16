//! HTTP server that lets Home Assistant (or anything else on the LAN)
//! drive the dashboard remotely.
//!
//! Routes:
//! - `POST /widget/<name>` — switch to the named widget
//! - `POST /blank/on`  — blank the screen
//! - `POST /blank/off` — unblank the screen
//! - `POST /blank/toggle` — toggle the blank state
//! - `POST /blank` with body `{"blanked": bool}` — explicit set (used by
//!   Home Assistant's `switch.rest` platform via `body_on`/`body_off`)
//! - `GET  /blank` → `{"blanked": bool}` — current state (used by
//!   Home Assistant's `switch.rest` polling)
//!
//! Every request must carry `Authorization: Bearer <token>`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::{get, post};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::config::RemoteControlConfig;

#[derive(Clone)]
struct AppState {
    name_to_id: Arc<HashMap<String, i32>>,
    token: Arc<String>,
    dashboard: slint::Weak<crate::Dashboard>,
}

/// Spawn the remote-control HTTP server.  The caller is responsible for
/// resolving the bearer token (typically via
/// [`crate::config::remote_control_token`]); the controller wires this
/// up in [`crate::widget::WidgetController::spawn_remote_control`].
pub fn spawn(
    config: RemoteControlConfig,
    token: String,
    name_to_id: HashMap<String, i32>,
    dashboard: slint::Weak<crate::Dashboard>,
) {
    let state = AppState {
        name_to_id: Arc::new(name_to_id),
        token: Arc::new(token),
        dashboard,
    };
    let listen = config.listen;

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            let app = Router::new()
                .route("/widget/:name", post(switch_widget))
                .route("/blank", get(get_blank).post(set_blank_body))
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

/// Returns `Err(401)` if the request's `Authorization` header does not
/// match `Bearer <token>`.
fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if provided == Some(state.token.as_str()) {
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

#[derive(Deserialize, Serialize)]
struct BlankStatus {
    blanked: bool,
}

async fn get_blank(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<BlankStatus>, StatusCode> {
    check_auth(&state, &headers)?;
    let (tx, rx) = oneshot::channel();
    let handle = state.dashboard.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let value = handle.upgrade().map(|d| d.get_blanked()).unwrap_or(false);
        let _ = tx.send(value);
    });
    let blanked = rx.await.unwrap_or(false);
    Ok(Json(BlankStatus { blanked }))
}

async fn set_blank_body(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BlankStatus>,
) -> Result<&'static str, StatusCode> {
    check_auth(&state, &headers)?;
    info!("Remote control: blank set blanked={}", body.blanked);
    let handle = state.dashboard.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(dashboard) = handle.upgrade() {
            dashboard.set_blanked(body.blanked);
        }
    });
    Ok("ok")
}
