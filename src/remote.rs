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

use crate::config::RemoteControlConfig;

/// Abstract interface for driving the dashboard from the remote-control
/// HTTP server.
///
/// Production code uses the impl on `slint::Weak<crate::Dashboard>`; tests
/// use [`MockDashboardProxy`].
pub trait DashboardProxy: Send + Sync {
    /// Switch to the widget with the given id.
    fn activate_widget(&self, id: i32);
    /// Set the blanked state.
    fn set_blanked(&self, value: bool);
    /// Get the current blanked state.
    fn get_blanked(&self) -> bool;
}

impl DashboardProxy for slint::Weak<crate::Dashboard> {
    fn activate_widget(&self, id: i32) {
        let handle = self.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(dashboard) = handle.upgrade() {
                dashboard.invoke_activate_widget(id);
            }
        });
    }

    fn set_blanked(&self, value: bool) {
        let handle = self.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(dashboard) = handle.upgrade() {
                dashboard.set_blanked(value);
            }
        });
    }

    fn get_blanked(&self) -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = self.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let value = handle.upgrade().map(|d| d.get_blanked()).unwrap_or(false);
            let _ = tx.send(value);
        });
        rx.recv().unwrap_or(false)
    }
}

#[derive(Clone)]
struct AppState {
    name_to_id: Arc<HashMap<String, i32>>,
    token: Arc<String>,
    dashboard: Arc<dyn DashboardProxy>,
}

/// Spawn the remote-control HTTP server.  The caller is responsible for
/// resolving the bearer token (typically via
/// [`crate::config::remote_control_token`]); the controller wires this
/// up in [`crate::widget::WidgetController::spawn_remote_control`].
pub fn spawn(
    config: RemoteControlConfig,
    token: String,
    name_to_id: HashMap<String, i32>,
    dashboard: Arc<dyn DashboardProxy>,
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
    state.dashboard.activate_widget(id);
    Ok("ok")
}

async fn set_blank(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(action): Path<String>,
) -> Result<&'static str, StatusCode> {
    check_auth(&state, &headers)?;
    let value = match action.as_str() {
        "on" => true,
        "off" => false,
        "toggle" => !state.dashboard.get_blanked(),
        _ => {
            warn!("Remote control: unknown blank action '{action}'");
            return Err(StatusCode::NOT_FOUND);
        }
    };
    info!("Remote control: blank {action}");
    state.dashboard.set_blanked(value);
    Ok("ok")
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
    let blanked = state.dashboard.get_blanked();
    Ok(Json(BlankStatus { blanked }))
}

async fn set_blank_body(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BlankStatus>,
) -> Result<&'static str, StatusCode> {
    check_auth(&state, &headers)?;
    info!("Remote control: blank set blanked={}", body.blanked);
    state.dashboard.set_blanked(body.blanked);
    Ok("ok")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Test double that records interactions instead of talking to Slint.
    struct MockDashboardProxy {
        activated_widget: AtomicI32,
        blanked: AtomicBool,
    }

    impl MockDashboardProxy {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                activated_widget: AtomicI32::new(-1),
                blanked: AtomicBool::new(false),
            })
        }
    }

    impl DashboardProxy for MockDashboardProxy {
        fn activate_widget(&self, id: i32) {
            self.activated_widget.store(id, Ordering::SeqCst);
        }

        fn set_blanked(&self, value: bool) {
            self.blanked.store(value, Ordering::SeqCst);
        }

        fn get_blanked(&self) -> bool {
            self.blanked.load(Ordering::SeqCst)
        }
    }

    fn test_app(proxy: Arc<dyn DashboardProxy>) -> Router {
        let mut name_to_id = HashMap::new();
        name_to_id.insert("clock".into(), 2);
        name_to_id.insert("weather".into(), 5);

        let state = AppState {
            name_to_id: Arc::new(name_to_id),
            token: Arc::new("secret".into()),
            dashboard: proxy,
        };

        Router::new()
            .route("/widget/:name", post(switch_widget))
            .route("/blank", get(get_blank).post(set_blank_body))
            .route("/blank/:action", post(set_blank))
            .with_state(state)
    }

    // ------------------------------------------------------------------
    // Auth tests (also exercised by the E2E tests below, but kept for
    // fast unit-level coverage).
    // ------------------------------------------------------------------

    #[test]
    fn check_auth_valid_token() {
        let state = AppState {
            name_to_id: Arc::new(HashMap::new()),
            token: Arc::new("secret".into()),
            dashboard: MockDashboardProxy::new(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer secret".parse().unwrap(),
        );
        assert!(check_auth(&state, &headers).is_ok());
    }

    #[test]
    fn check_auth_invalid_token() {
        let state = AppState {
            name_to_id: Arc::new(HashMap::new()),
            token: Arc::new("secret".into()),
            dashboard: MockDashboardProxy::new(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer wrong".parse().unwrap(),
        );
        assert_eq!(check_auth(&state, &headers), Err(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn check_auth_missing_header() {
        let state = AppState {
            name_to_id: Arc::new(HashMap::new()),
            token: Arc::new("secret".into()),
            dashboard: MockDashboardProxy::new(),
        };
        let headers = HeaderMap::new();
        assert_eq!(check_auth(&state, &headers), Err(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn check_auth_wrong_scheme() {
        let state = AppState {
            name_to_id: Arc::new(HashMap::new()),
            token: Arc::new("secret".into()),
            dashboard: MockDashboardProxy::new(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Basic dXNlcjpwYXNz".parse().unwrap(),
        );
        assert_eq!(check_auth(&state, &headers), Err(StatusCode::UNAUTHORIZED));
    }

    // ------------------------------------------------------------------
    // E2E tests using tower::ServiceExt::oneshot
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn e2e_switch_widget_ok() {
        let proxy = MockDashboardProxy::new();
        let app = test_app(proxy.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/widget/clock")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(proxy.activated_widget.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn e2e_switch_widget_unknown() {
        let app = test_app(MockDashboardProxy::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/widget/unknown")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn e2e_switch_widget_unauthorized() {
        let app = test_app(MockDashboardProxy::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/widget/clock")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn e2e_blank_on() {
        let proxy = MockDashboardProxy::new();
        let app = test_app(proxy.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/blank/on")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(proxy.get_blanked());
    }

    #[tokio::test]
    async fn e2e_blank_off() {
        let proxy = MockDashboardProxy::new();
        proxy.set_blanked(true);
        let app = test_app(proxy.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/blank/off")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(!proxy.get_blanked());
    }

    #[tokio::test]
    async fn e2e_blank_toggle() {
        let proxy = MockDashboardProxy::new();
        let app = test_app(proxy.clone());

        // toggle off → on
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/blank/toggle")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(proxy.get_blanked());

        // toggle on → off
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/blank/toggle")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!proxy.get_blanked());
    }

    #[tokio::test]
    async fn e2e_blank_unknown_action() {
        let app = test_app(MockDashboardProxy::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/blank/xyz")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn e2e_blank_body_set() {
        let proxy = MockDashboardProxy::new();
        let app = test_app(proxy.clone());

        let body = serde_json::to_string(&BlankStatus { blanked: true }).unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/blank")
                    .header("Authorization", "Bearer secret")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(proxy.get_blanked());
    }

    #[tokio::test]
    async fn e2e_get_blank() {
        let proxy = MockDashboardProxy::new();
        proxy.set_blanked(true);
        let app = test_app(proxy.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/blank")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let status: BlankStatus = serde_json::from_slice(&body).unwrap();
        assert!(status.blanked);
    }

    #[tokio::test]
    async fn e2e_get_blank_unauthorized() {
        let app = test_app(MockDashboardProxy::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/blank")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
