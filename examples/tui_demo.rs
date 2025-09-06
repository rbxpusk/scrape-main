use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};
use tokio::time::sleep;
use uuid::Uuid;

use twitch_chat_scraper::tui::{
    Action, AgentInfo, Dashboard, LogEntry, LogLevel, SystemMetrics, TUIMonitor,
};
use twitch_chat_scraper::agents::{AgentMetrics, AgentStatus};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // creating dashboard
    let mut dashboard = Dashboard::new();
    
    // adding sample log entries
    dashboard.add_log(LogEntry {
        timestamp: chrono::Utc::now(),
        level: LogLevel::Info,
        message: "TUI Demo started".to_string(),
        agent_id: None,
    });
    
    dashboard.add_log(LogEntry {
        timestamp: chrono::Utc::now(),
        level: LogLevel::Info,
        message: "Initializing agents...".to_string(),
        agent_id: None,
    });

    // running the app
    let res = run_app(&mut terminal, &mut dashboard).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    dashboard: &mut Dashboard,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(250);
    let mut message_count = 0u64;
    let start_time = Instant::now();

    // creating sample agents
    let sample_agents = vec![
        AgentInfo {
            id: Uuid::new_v4(),
            streamer: "shroud".to_string(),
            status: AgentStatus::Running,
            metrics: AgentMetrics {
                messages_scraped: 1250,
                uptime: Duration::from_secs(3600),
                error_count: 2,
                last_message_time: Some(chrono::Utc::now() - chrono::Duration::seconds(30)),
                network_latency: Duration::from_millis(45),
                memory_usage: 128 * 1024 * 1024, // 128 MB
                status: AgentStatus::Running,
            },
        },
        AgentInfo {
            id: Uuid::new_v4(),
            streamer: "ninja".to_string(),
            status: AgentStatus::Running,
            metrics: AgentMetrics {
                messages_scraped: 890,
                uptime: Duration::from_secs(2400),
                error_count: 0,
                last_message_time: Some(chrono::Utc::now() - chrono::Duration::seconds(5)),
                network_latency: Duration::from_millis(32),
                memory_usage: 95 * 1024 * 1024, // 95 MB
                status: AgentStatus::Running,
            },
        },
        AgentInfo {
            id: Uuid::new_v4(),
            streamer: "pokimane".to_string(),
            status: AgentStatus::Error("Connection timeout".to_string()),
            metrics: AgentMetrics {
                messages_scraped: 456,
                uptime: Duration::from_secs(1800),
                error_count: 5,
                last_message_time: Some(chrono::Utc::now() - chrono::Duration::minutes(10)),
                network_latency: Duration::from_millis(120),
                memory_usage: 87 * 1024 * 1024, // 87 MB
                status: AgentStatus::Error("Connection timeout".to_string()),
            },
        },
        AgentInfo {
            id: Uuid::new_v4(),
            streamer: "xqc".to_string(),
            status: AgentStatus::Starting,
            metrics: AgentMetrics {
                messages_scraped: 0,
                uptime: Duration::from_secs(30),
                error_count: 0,
                last_message_time: None,
                network_latency: Duration::from_millis(0),
                memory_usage: 45 * 1024 * 1024, // 45 MB
                status: AgentStatus::Starting,
            },
        },
    ];

    dashboard.update_agents(sample_agents.clone());

    loop {
        // handling events
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match dashboard.handle_input(Event::Key(key)) {
                    Ok(Action::Quit) => return Ok(()),
                    Ok(Action::Refresh) => {
                        dashboard.add_log(LogEntry {
                            timestamp: chrono::Utc::now(),
                            level: LogLevel::Info,
                            message: "Manual refresh triggered".to_string(),
                            agent_id: None,
                        });
                    }
                    Ok(Action::StopAgent(agent_id)) => {
                        dashboard.add_log(LogEntry {
                            timestamp: chrono::Utc::now(),
                            level: LogLevel::Warning,
                            message: format!("Stop agent requested for {}", agent_id),
                            agent_id: Some(agent_id),
                        });
                    }
                    Ok(Action::RestartAgent(agent_id)) => {
                        dashboard.add_log(LogEntry {
                            timestamp: chrono::Utc::now(),
                            level: LogLevel::Info,
                            message: format!("Restart agent requested for {}", agent_id),
                            agent_id: Some(agent_id),
                        });
                    }
                    Ok(Action::ShowHelp) => {
                        dashboard.add_log(LogEntry {
                            timestamp: chrono::Utc::now(),
                            level: LogLevel::Debug,
                            message: "Help popup shown".to_string(),
                            agent_id: None,
                        });
                    }
                    _ => {}
                }
            }
        }

        // updating metrics periodically
        if last_tick.elapsed() >= tick_rate {
            // Simulate message count increase
            message_count += rand::random::<u64>() % 10;
            
            // Simulate CPU and memory usage fluctuation
            let cpu_usage = 45.0 + (start_time.elapsed().as_secs() as f32 * 0.1).sin() * 15.0;
            let memory_usage = 2_000_000_000 + ((start_time.elapsed().as_secs() as f64 * 0.05).sin() * 500_000_000.0) as u64;
            
            let system_metrics = SystemMetrics {
                active_agents: sample_agents.iter().filter(|a| matches!(a.status, AgentStatus::Running)).count() as u32,
                total_messages: message_count,
                messages_per_second: 0.0, // Will be calculated by the dashboard
                cpu_usage,
                memory_usage,
                memory_total: 8_000_000_000, // 8 GB
                uptime: start_time.elapsed(),
            };
            
            dashboard.update_metrics(system_metrics);
            
            // Occasionally add log entries
            if rand::random::<u8>() % 20 == 0 {
                let log_levels = [LogLevel::Info, LogLevel::Warning, LogLevel::Error, LogLevel::Debug];
                let messages = [
                    "New chat message processed",
                    "Browser instance restarted",
                    "Network latency spike detected",
                    "Memory usage optimized",
                    "Agent performance within normal range",
                    "Configuration updated",
                ];
                
                dashboard.add_log(LogEntry {
                    timestamp: chrono::Utc::now(),
                    level: log_levels[rand::random::<usize>() % log_levels.len()],
                    message: messages[rand::random::<usize>() % messages.len()].to_string(),
                    agent_id: if rand::random::<bool>() { 
                        Some(sample_agents[rand::random::<usize>() % sample_agents.len()].id) 
                    } else { 
                        None 
                    },
                });
            }
            
            last_tick = Instant::now();
        }

        // rendering the ui
        terminal.draw(|f| {
            if let Err(e) = dashboard.render(f) {
                eprintln!("Render error: {}", e);
            }
        })?;

        
    }
}