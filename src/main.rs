
use std::sync::Arc;
use twitch_chat_scraper::config::{ConfigManager, FileConfigManager};
use twitch_chat_scraper::tui::{Dashboard, TUIMonitor};
use twitch_chat_scraper::scraper::SimpleTwitchScraper;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> twitch_chat_scraper::error::Result<()> {
    tracing_subscriber::fmt::init();

    let config_manager = Arc::new(FileConfigManager::new(PathBuf::from("config.toml")));
    let config = config_manager.load_config().await?;
    let config_arc = Arc::new(config);

    tracing::info!("Starting Twitch Chat Scraper");
    
    // creating output dir right away
    if let Err(e) = std::fs::create_dir_all(&config_arc.output.directory) {
        tracing::error!("Failed to create output directory: {}", e);
    } else {
        tracing::info!("Created output directory: {}", config_arc.output.directory.display());
    }
    
    // starting scraper in background
    let scraper_config = config_arc.clone();
    tokio::spawn(async move {
        let scraper = SimpleTwitchScraper::new(
            scraper_config.output.directory.clone(),
            scraper_config.streamers.clone()
        );
        
        if let Err(e) = scraper.start_scraping().await {
            tracing::error!("Scraper error: {}", e);
        }
    });
    
    // running the tui
    let config_for_tui = config_arc.clone();
    let config_manager_for_tui = config_manager.clone();
    if let Err(e) = run_tui_without_orchestrator(config_for_tui).await {
        eprintln!("TUI error: {}", e);
    }

    tracing::info!("Twitch Chat Scraper stopped.");
    Ok(())
}

async fn run_tui_without_orchestrator(config: Arc<twitch_chat_scraper::config::Config>) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::{event, terminal, execute};
    use ratatui::prelude::{CrosstermBackend, Terminal};
    use std::io;
    use std::time::Duration;

    tracing::info!("Initializing TUI...");

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut dashboard = Dashboard::new();
    dashboard.set_config((*config).clone());
    
    // adding initial logs
    dashboard.add_log(twitch_chat_scraper::tui::LogEntry {
        timestamp: chrono::Utc::now(),
        level: twitch_chat_scraper::tui::LogLevel::Info,
        message: "TUI started successfully! Press 'q' to quit".to_string(),
        agent_id: None,
    });
    
    dashboard.add_log(twitch_chat_scraper::tui::LogEntry {
        timestamp: chrono::Utc::now(),
        level: twitch_chat_scraper::tui::LogLevel::Info,
        message: "Simple HTTP scraper started for all streamers".to_string(),
        agent_id: None,
    });

    dashboard.add_log(twitch_chat_scraper::tui::LogEntry {
        timestamp: chrono::Utc::now(),
        level: twitch_chat_scraper::tui::LogLevel::Info,
        message: format!("Scraping {} streamers: {}", config.streamers.len(), config.streamers.join(", ")),
        agent_id: None,
    });

    dashboard.add_log(twitch_chat_scraper::tui::LogEntry {
        timestamp: chrono::Utc::now(),
        level: twitch_chat_scraper::tui::LogLevel::Info,
        message: format!("Output directory: {}", config.output.directory.display()),
        agent_id: None,
    });

    tracing::info!("TUI initialized, entering main loop");

    let mut should_quit = false;
    let start_time = std::time::Instant::now();

    while !should_quit {
        // handling input with timeout
        if event::poll(Duration::from_millis(100))? {
            let input_event = event::read()?;
            
            // handling ctrl+c manually
            if let event::Event::Key(key) = input_event {
                if key.code == event::KeyCode::Char('c') && key.modifiers.contains(event::KeyModifiers::CONTROL) {
                    dashboard.add_log(twitch_chat_scraper::tui::LogEntry {
                        timestamp: chrono::Utc::now(),
                        level: twitch_chat_scraper::tui::LogLevel::Info,
                        message: "Received Ctrl+C, shutting down...".to_string(),
                        agent_id: None,
                    });
                    should_quit = true;
                    continue;
                }
            }
            
            match dashboard.handle_input(input_event)? {
                twitch_chat_scraper::tui::Action::Quit => {
                    dashboard.add_log(twitch_chat_scraper::tui::LogEntry {
                        timestamp: chrono::Utc::now(),
                        level: twitch_chat_scraper::tui::LogLevel::Info,
                        message: "Quit requested, shutting down...".to_string(),
                        agent_id: None,
                    });
                    should_quit = true;
                }
                _ => {}
            }
        }

        // updating dashboard data
        let system_metrics = twitch_chat_scraper::tui::SystemMetrics {
            active_agents: 0,
            total_messages: 0,
            messages_per_second: 0.0,
            cpu_usage: 0.0,
            memory_usage: 0,
            memory_total: 1,
            uptime: start_time.elapsed(),
        };
        dashboard.update_metrics(system_metrics);
        dashboard.update_agents(vec![]);

        // rendering the dashboard
        terminal.draw(|f| {
            if let Err(e) = dashboard.render(f) {
                tracing::error!("Render error: {}", e);
            }
        })?;

        // small delay to save cpu
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tracing::info!("Cleaning up TUI...");
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
