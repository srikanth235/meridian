use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use meridian_config::ReloadHandle;
use meridian_orchestrator::OrchestratorHandle;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    orch: OrchestratorHandle,
    workflow: ReloadHandle,
}

pub async fn serve(
    addr: SocketAddr,
    orch: OrchestratorHandle,
    workflow: ReloadHandle,
    static_dir: Option<PathBuf>,
) -> std::io::Result<()> {
    let state = AppState { orch, workflow };

    let api = Router::new()
        .route("/snapshot", get(get_snapshot))
        .route("/workflow", get(get_workflow))
        .route("/health", get(get_health))
        .route("/sessions/:issue_id/log", get(get_session_log))
        .route("/control/pause", post(post_pause))
        .route("/control/resume", post(post_resume))
        .route("/harnesses/refresh", post(post_refresh_harnesses))
        .route("/harnesses/concurrency", post(post_set_concurrency))
        .route("/repos/refresh", post(post_refresh_repos))
        .route("/repos/connect", post(post_set_repo_connected))
        .route("/repos/add", post(post_add_repo))
        .route("/ws", get(ws_upgrade));

    let mut app = Router::new()
        .nest("/api", api)
        .with_state(state)
        .layer(CorsLayer::very_permissive())
        .layer(TraceLayer::new_for_http());

    if let Some(dir) = static_dir {
        if dir.is_dir() {
            info!(path = %dir.display(), "serving static renderer assets");
            app = app.fallback_service(ServeDir::new(dir));
        } else {
            warn!(path = %dir.display(), "static dir not found; UI disabled");
        }
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "meridian http server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_snapshot(State(s): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(s.orch.snapshot()).unwrap_or_default())
}

async fn get_workflow(State(s): State<AppState>) -> Json<serde_json::Value> {
    let wf = s.workflow.current();
    Json(serde_json::json!({
        "source_path": wf.source_path,
        "config": wf.raw_config,
        "prompt_template": wf.prompt_template,
    }))
}

async fn get_health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

async fn post_pause(State(s): State<AppState>) -> impl IntoResponse {
    s.orch.set_paused(Some(true));
    (StatusCode::OK, Json(serde_json::json!({"paused": true})))
}

async fn post_resume(State(s): State<AppState>) -> impl IntoResponse {
    s.orch.set_paused(Some(false));
    (StatusCode::OK, Json(serde_json::json!({"paused": false})))
}

async fn post_refresh_harnesses(State(s): State<AppState>) -> impl IntoResponse {
    let harnesses = s.orch.refresh_harnesses().await;
    (StatusCode::OK, Json(serde_json::json!({ "harnesses": harnesses })))
}

#[derive(serde::Deserialize)]
struct SetConcurrencyBody {
    id: String,
    concurrency: u32,
}

async fn post_set_concurrency(
    State(s): State<AppState>,
    Json(body): Json<SetConcurrencyBody>,
) -> impl IntoResponse {
    match s.orch.set_harness_concurrency(&body.id, body.concurrency).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

async fn post_refresh_repos(State(s): State<AppState>) -> impl IntoResponse {
    let repos = s.orch.refresh_repos().await;
    (StatusCode::OK, Json(serde_json::json!({ "repos": repos })))
}

#[derive(serde::Deserialize)]
struct SetRepoConnectedBody {
    slug: String,
    connected: bool,
}

async fn post_set_repo_connected(
    State(s): State<AppState>,
    Json(body): Json<SetRepoConnectedBody>,
) -> impl IntoResponse {
    match s.orch.set_repo_connected(&body.slug, body.connected).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

#[derive(serde::Deserialize)]
struct AddRepoBody {
    slug: String,
}

async fn post_add_repo(
    State(s): State<AppState>,
    Json(body): Json<AddRepoBody>,
) -> impl IntoResponse {
    match s.orch.add_repo_by_slug(&body.slug).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

async fn get_session_log(
    State(s): State<AppState>,
    Path(issue_id): Path<String>,
) -> impl IntoResponse {
    match s.orch.session_log(&issue_id) {
        Some(log) => (StatusCode::OK, Json(serde_json::to_value(log).unwrap_or_default())),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "no log for issue"})),
        ),
    }
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(s): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_loop(socket, s))
}

async fn ws_loop(mut socket: WebSocket, state: AppState) {
    let mut events = state.orch.subscribe_events();
    let mut ticker = tokio::time::interval(Duration::from_secs(2));

    async fn send_snap(sock: &mut WebSocket, snap: &meridian_orchestrator::Snapshot) -> bool {
        let payload = serde_json::to_string(snap).unwrap_or_else(|_| "{}".into());
        sock.send(Message::Text(payload)).await.is_ok()
    }

    let snap = state.orch.snapshot();
    if !send_snap(&mut socket, &snap).await {
        return;
    }

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let snap = state.orch.snapshot();
                if !send_snap(&mut socket, &snap).await { break; }
            }
            _ = events.recv() => {
                let snap = state.orch.snapshot();
                if !send_snap(&mut socket, &snap).await { break; }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

// Convenience re-exports.
pub use meridian_orchestrator::{Orchestrator, OrchestratorHandle as Handle, Snapshot};
pub fn _arc<T>(t: T) -> Arc<T> { Arc::new(t) }
