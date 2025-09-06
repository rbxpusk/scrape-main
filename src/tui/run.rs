use crate::agents::AgentOrchestrator;
use crate::tui::{Action, Dashboard, TUIMonitor};
use anyhow::Result;
use crossterm::{event, terminal, execute};
use tokio::signal;
use ratatui::prelude::{CrosstermBackend, Terminal};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub async fn run_tui(orchestrator: Arc<RwLock<AgentOrchestrator>>, config: Arc<crate::config::Config>, config_manager: Arc<dyn crate::config::ConfigManager + Send + Sync>) -> Result<()> {
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut dashboard = Dashboard::new();
    dashboard.set_config_manager(config_manager);
    
    // add initial log entries
    dashboard.add_log(crate::tui::LogEntry {
        timestamp: chrono::Utc::now(),
        level: crate::tui::LogLevel::Info,
        message: "Twitch Chat Scraper started".to_string(),
        agent_id: None,
    });
    
    dashboard.add_log(crate::tui::LogEntry {
        timestamp: chrono::Utc::now(),
        level: crate::tui::LogLevel::Info,
        message: "Loading configuration...".to_string(),
        agent_id: None,
    });

    dashboard.add_log(crate::tui::LogEntry {
        timestamp: chrono::Utc::now(),
        level: crate::tui::LogLevel::Info,
        message: format!("Configured streamers: {}", config.streamers.join(", ")),
        agent_id: None,
    });

    // Set the config in dashboard
    dashboard.set_config((*config).clone());

    // set up signal handling for ctrl+c
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;

    loop {
        tokio::select! {
            // handle ctrl+c and sigterm
            _ = sigint.recv() => {
                dashboard.add_log(crate::tui::LogEntry {
                    timestamp: chrono::Utc::now(),
                    level: crate::tui::LogLevel::Info,
                    message: "Received interrupt signal (Ctrl+C), shutting down...".to_string(),
                    agent_id: None,
                });
                break;
            }
            _ = sigterm.recv() => {
                dashboard.add_log(crate::tui::LogEntry {
                    timestamp: chrono::Utc::now(),
                    level: crate::tui::LogLevel::Info,
                    message: "Received termination signal, shutting down...".to_string(),
                    agent_id: None,
                });
                break;
            }
            // handle keyboard input
            input_result = async {
                if event::poll(Duration::from_millis(100))? {
                    Ok::<Option<crossterm::event::Event>, anyhow::Error>(Some(event::read()?))
                } else {
                    Ok(None)
                }
            } => {
                match input_result? {
                    Some(input_event) => {
                        if let Action::Quit = dashboard.handle_input(input_event).map_err(anyhow::Error::from)? {
                            break;
                        }
                    }
                    None => {} // No input, continue
                }
            }
            // update dashboard data
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                // Update dashboard with real data from orchestrator
                let orchestrator_read = orchestrator.read().await;
                
                // Get orchestrator status which includes system metrics
                let orchestrator_status = orchestrator_read.get_status().await;
                
                // Update system metrics
                let system_metrics = crate::tui::SystemMetrics {
                    active_agents: orchestrator_status.active_agents as u32,
                    total_messages: orchestrator_status.system_metrics.total_messages_scraped,
                    messages_per_second: 0.0, // Calculate from recent data
                    cpu_usage: orchestrator_status.system_metrics.cpu_usage,
                    memory_usage: orchestrator_status.system_metrics.memory_usage,
                    memory_total: orchestrator_status.system_metrics.memory_total,
                    uptime: orchestrator_status.system_metrics.timestamp.elapsed().unwrap_or_default(),
                };
                dashboard.update_metrics(system_metrics);
                
                // Get real agent information
                let mut agents_info = Vec::new();
                let assignments = orchestrator_read.agent_assignments.read().await;
                for assignment in assignments.values() {
                    // Get real agent status and metrics
                    let agent_status = orchestrator_read.get_agent_status(assignment.agent_id).await
                        .unwrap_or(crate::agents::AgentStatus::Idle);
                    let agent_metrics = orchestrator_read.get_agent_metrics(assignment.agent_id).await;
                    
                    let agent_info = crate::tui::AgentInfo {
                        id: assignment.agent_id,
                        channel: assignment.streamer.clone(),
                        status: agent_status,
                        uptime: agent_metrics.as_ref().map(|m| m.uptime).unwrap_or_default(),
                        messages_per_second: 0.0, // Calculate from metrics if available
                        error_count: agent_metrics.as_ref().map(|m| m.error_count).unwrap_or(assignment.retry_attempts),
                        alert_id: None,
                    };
                    agents_info.push(agent_info);
                }
                
                dashboard.update_agents(agents_info);

                // Render the dashboard
                let mut render_result = Ok(());
                terminal.draw(|f| {
                    render_result = dashboard.render(f);
                })?;
                render_result?;
            }
        }
    }

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
