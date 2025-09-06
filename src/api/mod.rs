pub mod auth;
pub mod dashboard;
pub mod stream;

use axum::{extract::State, response::Json, routing::{get, post}, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::agents::{AgentId, AgentOrchestrator, AgentStatus, AgentMetrics, OrchestratorStatus};
use crate::error::Result;
use crate::config::Config;

#[derive(Serialize)]
pub struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}

pub async fn start_api_server(
    orchestrator: Arc<RwLock<AgentOrchestrator>>,
    config: Arc<Config>,
) -> Result<()> {
    let stream_router = stream::create_stream_router(orchestrator.clone(), config.clone()).await;

    let app = Router::new()
        .route("/status", get(get_orchestrator_status))
        .route("/agents", get(list_agents))
        .route("/agents/:id/status", get(get_agent_status))
        .route("/agents/:id/metrics", get(get_agent_metrics))
        .route("/agents/:id/start", post(start_agent))
        .route("/agents/:id/stop", post(stop_agent))
        .route("/agents/:id/restart", post(restart_agent))
        .route("/agents", post(create_agent))
        .merge(stream_router)
        .with_state(orchestrator);

    let addr = format!("0.0.0.0:{}", config.monitoring.api_port);
    info!("API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("API server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

pub async fn start_dashboard_server(
    orchestrator: Arc<RwLock<AgentOrchestrator>>,
    config: Arc<Config>,
) -> Result<()> {
    let dashboard_port = config.monitoring.dashboard_port.unwrap_or(8888);
    let app = dashboard::create_dashboard_router().with_state(orchestrator);

    let addr = format!("0.0.0.0:{}", dashboard_port);
    info!("Dashboard server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    streamer: String,
    priority: Option<u8>,
}

async fn create_agent(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    Json(payload): Json<CreateAgentRequest>,
) -> Json<ApiResponse<AgentId>> {
    let mut orchestrator_guard = orchestrator.write().await;
    match orchestrator_guard.spawn_agent(&payload.streamer, payload.priority.unwrap_or(0)).await {
        Ok(agent_id) => Json(ApiResponse::success(agent_id)),
        Err(e) => Json(ApiResponse::error(format!("Failed to create agent: {}", e))),
    }
}

async fn start_agent(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    axum::extract::Path(agent_id): axum::extract::Path<AgentId>,
) -> Json<ApiResponse<String>> {
    let mut orchestrator_guard = orchestrator.write().await;
    match orchestrator_guard.restart_agent(agent_id).await { // Restarting is effectively starting if stopped
        Ok(_) => Json(ApiResponse::success(format!("Agent {} started/restarted successfully", agent_id))),
        Err(e) => Json(ApiResponse::error(format!("Failed to start/restart agent {}: {}", agent_id, e))),
    }
}

async fn stop_agent(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    axum::extract::Path(agent_id): axum::extract::Path<AgentId>,
) -> Json<ApiResponse<String>> {
    let mut orchestrator_guard = orchestrator.write().await;
    match orchestrator_guard.stop_agent(agent_id).await {
        Ok(_) => Json(ApiResponse::success(format!("Agent {} stopped successfully", agent_id))),
        Err(e) => Json(ApiResponse::error(format!("Failed to stop agent {}: {}", agent_id, e))),
    }
}

async fn restart_agent(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    axum::extract::Path(agent_id): axum::extract::Path<AgentId>,
) -> Json<ApiResponse<String>> {
    let mut orchestrator_guard = orchestrator.write().await;
    match orchestrator_guard.restart_agent(agent_id).await {
        Ok(_) => Json(ApiResponse::success(format!("Agent {} restarted successfully", agent_id))),
        Err(e) => Json(ApiResponse::error(format!("Failed to restart agent {}: {}", agent_id, e))),
    }
}

async fn get_orchestrator_status(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
) -> Json<ApiResponse<OrchestratorStatus>> {
    let orchestrator_guard = orchestrator.read().await;
    let status = orchestrator_guard.get_status().await;
    Json(ApiResponse::success(status))
}

async fn list_agents(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
) -> Json<ApiResponse<Vec<AgentId>>> {
    let orchestrator_guard = orchestrator.read().await;
    let active_agents = orchestrator_guard.get_active_agents().await;
    Json(ApiResponse::success(active_agents))
}

async fn get_agent_status(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    axum::extract::Path(agent_id): axum::extract::Path<AgentId>,
) -> Json<ApiResponse<AgentStatus>> {
    let orchestrator_guard = orchestrator.read().await;
    match orchestrator_guard.get_agent_status(agent_id).await {
        Some(status) => Json(ApiResponse::success(status)),
        None => Json(ApiResponse::error(format!("Agent {} not found", agent_id))),
    }
}

async fn get_agent_metrics(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
    axum::extract::Path(agent_id): axum::extract::Path<AgentId>,
) -> Json<ApiResponse<AgentMetrics>> {
    let orchestrator_guard = orchestrator.read().await;
    match orchestrator_guard.get_agent_metrics(agent_id).await {
        Some(metrics) => Json(ApiResponse::success(metrics)),
        None => Json(ApiResponse::error(format!("Agent {} not found", agent_id))),
    }
}
