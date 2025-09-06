use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::agents::AgentOrchestrator;

pub fn create_dashboard_router() -> Router<Arc<RwLock<AgentOrchestrator>>> {
    Router::new()
        .route("/", get(dashboard_html))
        .route("/api/stats", get(dashboard_stats))
}

async fn dashboard_html() -> impl IntoResponse {
    Html(include_str!("../static/dashboard.html"))
}

async fn dashboard_stats(
    State(orchestrator): State<Arc<RwLock<AgentOrchestrator>>>,
) -> Result<impl IntoResponse, StatusCode> {
    let orchestrator = orchestrator.read().await;
    let status = orchestrator.get_status().await;
    
    Ok(axum::Json(serde_json::json!({
        "active_agents": status.active_agents,
        "total_agents_spawned": status.total_agents_spawned,
        "system_metrics": status.system_metrics,
        "agent_assignments": status.agent_assignments,
        "error_count": status.error_count,
        "timestamp": chrono::Utc::now()
    })))
}