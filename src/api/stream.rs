use axum::{
    extract::State,
    middleware,
    response::{
        sse::{Event, Sse},

    },
    routing::{get, post},
    Json, Router,
};
use futures::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use tower_http::cors::{Any, CorsLayer};

use crate::agents::AgentOrchestrator;
use crate::api::auth::auth_middleware;
use crate::config::Config;

pub async fn create_stream_router(
    _orchestrator: Arc<RwLock<AgentOrchestrator>>,
    config: Arc<Config>,
) -> Router<Arc<RwLock<AgentOrchestrator>>> {
    let protected_routes = Router::new()
        .route("/agents/status", get(get_agents_status))
        .route("/agents/start", post(start_agent))
        .route("/agents/stop", post(stop_agent))
        .route_layer(middleware::from_fn_with_state(
            config.clone(),
            auth_middleware,
        ));

    Router::new()
        .route("/stream/status", get(status_handler))
        .route("/stream", get(sse_handler))
        .merge(protected_routes)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
}

async fn status_handler() -> &'static str {
    "ok"
}

async fn get_agents_status(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
) -> Json<serde_json::Value> {
    let orchestrator = orchestrator.read().await;
    let status = orchestrator.get_status().await;
    Json(serde_json::to_value(status).unwrap_or_default())
}

async fn start_agent(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    Json(payload): Json<serde_json::Value>,
) -> &'static str {
    let streamer = payload["streamer"].as_str().unwrap_or_default();
    if streamer.is_empty() {
        return "Missing streamer name";
    }
    let mut orchestrator = orchestrator.write().await;
    match orchestrator.spawn_agent(streamer, 0).await {
        Ok(_) => "Agent starting",
        Err(_) => "Failed to start agent",
    }
}

async fn stop_agent(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    Json(payload): Json<serde_json::Value>,
) -> &'static str {
    let streamer_to_stop = payload["streamer"].as_str().unwrap_or_default();
    if streamer_to_stop.is_empty() {
        return "Missing streamer name";
    }

    let agent_id_to_stop = {
        let orchestrator_read_guard = orchestrator.read().await;
        let assignments = orchestrator_read_guard.agent_assignments.read().await;
        assignments.iter().find_map(|(id, assignment)| {
            if assignment.streamer == streamer_to_stop {
                Some(*id)
            } else {
                None
            }
        })
    };

    if let Some(agent_id) = agent_id_to_stop {
        let mut orchestrator_write_guard = orchestrator.write().await;
        if orchestrator_write_guard.stop_agent(agent_id).await.is_ok() {
            "Agent stopping"
        } else {
            "Failed to stop agent"
        }
    } else {
        "Agent not found"
    }
}

async fn sse_handler(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let orchestrator = orchestrator.read().await;
    let mut rx = orchestrator.subscribe_to_chat_messages();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    let json = serde_json::to_string(&msg).unwrap();
                    yield Ok(Event::default().data(json));
                }
                Err(e) => {
                    eprintln!("SSE stream error: {}", e);
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive-text"),
    )
}

