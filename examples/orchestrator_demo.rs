use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error};

use twitch_chat_scraper::agents::AgentOrchestrator;
use twitch_chat_scraper::browser::BrowserManager;
use twitch_chat_scraper::browser::stealth::StealthConfig;
use twitch_chat_scraper::config::{Config, FileConfigManager, ConfigManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::init();

    info!("Starting Twitch Chat Scraper Orchestrator Demo");

    // creating config
    let mut config = Config::default();
    config.streamers = vec![
        "shroud".to_string(),
        "ninja".to_string(),
        "pokimane".to_string(),
    ];
    config.agents.max_concurrent = 3;

    // setting up stealth config
    let stealth_config = StealthConfig::default();

    // creating browser manager
    info!("Creating browser manager...");
    let browser_manager = match BrowserManager::new(
        config.agents.max_concurrent,
        stealth_config,
        vec![], // No proxies for demo
    ).await {
        Ok(manager) => Arc::new(manager),
        Err(e) => {
            error!("Failed to create browser manager: {}", e);
            return Err(e);
        }
    };

    // creating orchestrator
    info!("Creating agent orchestrator...");
    let mut orchestrator = AgentOrchestrator::new(config, browser_manager);

    // setting up config manager
    let config_path = std::path::PathBuf::from("config.toml");
    let config_manager = Arc::new(FileConfigManager::new(config_path));

    // starting the orchestrator
    info!("Starting orchestrator...");
    if let Err(e) = orchestrator.start(config_manager).await {
        error!("Failed to start orchestrator: {}", e);
        return Err(e);
    }

    // subscribing to messages
    let mut message_rx = orchestrator.subscribe_to_messages();

    // starting monitor task
    let message_monitor = tokio::spawn(async move {
        while let Ok(message) = message_rx.recv().await {
            match message {
                twitch_chat_scraper::agents::AgentMessage::StatusUpdate { agent_id, status } => {
                    info!("Agent {} status update: {:?}", agent_id, status);
                }
                twitch_chat_scraper::agents::AgentMessage::ResourceAlert { agent_id, alert } => {
                    warn!("Resource alert from agent {}: {}", agent_id, alert);
                }
                twitch_chat_scraper::agents::AgentMessage::Error { agent_id, error } => {
                    error!("Error from agent {}: {}", agent_id, error);
                }
                _ => {
                    info!("Received message: {:?}", message);
                }
            }
        }
    });

    // running demo for 30 seconds
    info!("Running orchestrator demo for 30 seconds...");
    
    // checking status every 5 seconds
    for i in 0..6 {
        sleep(Duration::from_secs(5)).await;
        
        let status = orchestrator.get_status().await;
        info!("Orchestrator Status ({}s):", i * 5);
        info!("  Active agents: {}", status.active_agents);
        info!("  Total agents spawned: {}", status.total_agents_spawned);
        info!("  Error count: {}", status.error_count);
        info!("  Uptime: {:?}", status.uptime);
        info!("  CPU usage: {:.1}%", status.system_metrics.cpu_usage);
        info!("  Memory usage: {} MB", status.system_metrics.memory_usage / 1024 / 1024);
        info!("  Total messages scraped: {}", status.system_metrics.total_messages_scraped);

        // getting active agents
        let active_agents = orchestrator.get_active_agents().await;
        info!("  Active agent IDs: {:?}", active_agents);

        // getting performance metrics
        let performance_metrics = orchestrator.get_agent_performance_metrics().await;
        for (agent_id, metrics) in performance_metrics {
            info!("  Agent {} metrics:", agent_id);
            info!("    Messages scraped: {}", metrics.messages_scraped);
            info!("    Error count: {}", metrics.error_count);
            info!("    Uptime: {:?}", metrics.uptime);
            info!("    Status: {:?}", metrics.status);
        }
    }

    // testing config update
    info!("Testing configuration update...");
    let mut new_config = Config::default();
    new_config.streamers = vec!["xqc".to_string(), "summit1g".to_string()];
    new_config.agents.max_concurrent = 2;

    if let Err(e) = orchestrator.update_config(new_config).await {
        warn!("Failed to update configuration: {}", e);
    } else {
        info!("Configuration updated successfully");
    }

    // waiting for config change effects
    sleep(Duration::from_secs(10)).await;
    
    let final_status = orchestrator.get_status().await;
    info!("Final Status:");
    info!("  Active agents: {}", final_status.active_agents);
    info!("  Agent assignments: {:?}", final_status.agent_assignments);

    // stopping the orchestrator
    info!("Stopping orchestrator...");
    if let Err(e) = orchestrator.stop().await {
        error!("Failed to stop orchestrator: {}", e);
    } else {
        info!("Orchestrator stopped successfully");
    }

    // stopping the monitor
    message_monitor.abort();

    info!("Demo completed");
    Ok(())
}