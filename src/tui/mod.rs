use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Tabs, Wrap},
    text::{Line, Span},
    Frame,
};
use std::collections::VecDeque;
use std::path::PathBuf;

use crate::agents::{AgentId, AgentStatus};

pub mod run;
pub use run::run_tui;

// Helper functions for AgentStatus
impl AgentStatus {
    fn symbol(&self) -> &'static str {
        match self {
            AgentStatus::Idle => "⏸",
            AgentStatus::Starting => "⏳",
            AgentStatus::Running => "▶",
            AgentStatus::Stopping => "⏹",
            AgentStatus::Stopped => "⏹",
            AgentStatus::Error(_) => "❌",
        }
    }

    fn color(&self) -> Color {
        match self {
            AgentStatus::Idle => Color::Yellow,
            AgentStatus::Starting => Color::Cyan,
            AgentStatus::Running => Color::Green,
            AgentStatus::Stopping => Color::Red,
            AgentStatus::Stopped => Color::Gray,
            AgentStatus::Error(_) => Color::Red,
        }
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Idle => write!(f, "Idle"),
            AgentStatus::Starting => write!(f, "Starting"),
            AgentStatus::Running => write!(f, "Running"),
            AgentStatus::Stopping => write!(f, "Stopping"),
            AgentStatus::Stopped => write!(f, "Stopped"),
            AgentStatus::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub active_agents: u32,
    pub total_messages: u64,
    pub messages_per_second: f64,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub memory_total: u64,
    pub uptime: std::time::Duration,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: AgentId,
    pub channel: String,
    pub status: AgentStatus,
    pub uptime: std::time::Duration,
    pub messages_per_second: f64,
    pub error_count: u32,
    pub alert_id: Option<u64>,
}

pub enum Action {
    Continue,
    Quit,
}

pub trait TUIMonitor {
    fn render(&mut self, frame: &mut Frame) -> Result<()>;
    fn handle_input(&mut self, event: Event) -> Result<Action>;
    fn update_metrics(&mut self, metrics: SystemMetrics);
    fn update_agents(&mut self, agents: Vec<AgentInfo>);
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Agents,
    Logs,
    Performance,
    Alerts,
    Config,
}

impl Tab {
    fn title(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Agents => "Agents",
            Tab::Logs => "Logs",
            Tab::Performance => "Performance",
            Tab::Alerts => "Alerts",
            Tab::Config => "Config",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub message: String,
    pub agent_id: Option<AgentId>,
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub id: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: AlertLevel,
    pub message: String,
    pub agent_id: Option<AgentId>,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

impl AlertLevel {
    fn color(&self) -> Color {
        match self {
            AlertLevel::Info => Color::Blue,
            AlertLevel::Warning => Color::Yellow,
            AlertLevel::Critical => Color::Red,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            AlertLevel::Info => "ℹ",
            AlertLevel::Warning => "⚠",
            AlertLevel::Critical => "❌",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceData {
    pub timestamp: std::time::Instant,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub messages_per_second: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Debug,
}

impl LogLevel {
    fn color(&self) -> Color {
        match self {
            LogLevel::Info => Color::Green,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
            LogLevel::Debug => Color::Cyan,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            LogLevel::Info => "ℹ",
            LogLevel::Warning => "⚠",
            LogLevel::Error => "❌",
            LogLevel::Debug => "🐛",
        }
    }
}

// A simple theming struct
pub struct CustomTheme {
    pub text_color: Color,
    pub accent_color: Color,
    pub border_color: Color,
    pub background_color: Color,
}

impl Default for CustomTheme {
    fn default() -> Self {
        Self {
            text_color: Color::White,
            accent_color: Color::Cyan,
            border_color: Color::White,
            background_color: Color::Black,
        }
    }
}

pub struct Dashboard {
    // Core state
    metrics: SystemMetrics,
    agents: Vec<AgentInfo>,
    logs: Vec<LogEntry>,
    alerts: Vec<Alert>,
    
    // UI state
    current_tab: Tab,
    show_help: bool,
    agent_table_state: TableState,
    log_list_state: ListState,
    
    // Performance tracking
    performance_history: VecDeque<PerformanceData>,
    last_message_count: u64,
    last_update_time: std::time::Instant,
    
    // Alert management
    next_alert_id: u64,
    
    // Config editing
    config: Option<crate::config::Config>,
    config_editing: bool,
    config_field_index: usize,
    
    // Theming
    theme: CustomTheme,
    custom_css_path: Option<PathBuf>,
}

impl Dashboard {
    pub fn new() -> Self {
        Self {
            metrics: SystemMetrics {
                active_agents: 0,
                total_messages: 0,
                messages_per_second: 0.0,
                cpu_usage: 0.0,
                memory_usage: 0,
                memory_total: 1,
                uptime: std::time::Duration::new(0, 0),
            },
            agents: Vec::new(),
            logs: Vec::new(),
            alerts: Vec::new(),
            current_tab: Tab::Overview,
            show_help: false,
            agent_table_state: TableState::default(),
            log_list_state: ListState::default(),
            performance_history: VecDeque::new(),
            last_message_count: 0,
            last_update_time: std::time::Instant::now(),
            next_alert_id: 1,
            config: None,
            config_editing: false,
            config_field_index: 0,
            theme: CustomTheme::default(),
            custom_css_path: None,
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);
        if self.logs.len() > 1000 {
            self.logs.remove(0);
        }
    }

    pub fn add_alert(&mut self, level: AlertLevel, message: String, agent_id: Option<AgentId>) {
        let alert = Alert {
            id: self.next_alert_id,
            timestamp: chrono::Utc::now(),
            level,
            message,
            agent_id,
            acknowledged: false,
        };
        self.alerts.push(alert);
        self.next_alert_id += 1;
    }

    pub fn set_config(&mut self, config: crate::config::Config) {
        self.config = Some(config);
    }

    fn render_overview(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Min(0),
            ])
            .split(area);

        // System metrics
        let metrics_text = format!(
            "Active Agents: {} | Total Messages: {} | Messages/sec: {:.2} | CPU: {:.1}% | Memory: {} MB",
            self.metrics.active_agents,
            self.metrics.total_messages,
            self.metrics.messages_per_second,
            self.metrics.cpu_usage,
            self.metrics.memory_usage / 1024 / 1024
        );
        let metrics = Paragraph::new(metrics_text)
            .block(Block::default().title("System Metrics").borders(Borders::ALL));
        frame.render_widget(metrics, chunks[0]);

        // Agent summary
        let agent_summary = format!(
            "Total Agents: {}\nRunning: {}\nIdle: {}\nError: {}",
            self.agents.len(),
            self.agents.iter().filter(|a| matches!(a.status, AgentStatus::Running)).count(),
            self.agents.iter().filter(|a| matches!(a.status, AgentStatus::Idle)).count(),
            self.agents.iter().filter(|a| matches!(a.status, AgentStatus::Error(_))).count(),
        );
        let summary = Paragraph::new(agent_summary)
            .block(Block::default().title("Agent Summary").borders(Borders::ALL));
        frame.render_widget(summary, chunks[1]);

        // Recent activity
        let activity_items: Vec<ListItem> = self.logs.iter()
            .rev()
            .take(chunks[2].height.saturating_sub(2) as usize)
            .map(|log| {
                ListItem::new(format!(
                    "[{}] {}: {}",
                    log.timestamp.format("%H:%M:%S"),
                    log.level.symbol(),
                    log.message
                ))
            })
            .collect();

        let activity_list = List::new(activity_items)
            .block(Block::default().title("Recent Activity").borders(Borders::ALL));
        frame.render_widget(activity_list, chunks[2]);
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["ID", "Channel", "Status", "Uptime", "Msgs/s", "Errors"]
            .iter()
            .map(|h| ratatui::widgets::Cell::from(*h).style(Style::default().fg(Color::Yellow)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows = self.agents.iter().map(|agent| {
            let uptime = format_duration(agent.uptime);
            Row::new(vec![
                ratatui::widgets::Cell::from(agent.id.to_string()),
                ratatui::widgets::Cell::from(agent.channel.clone()),
                ratatui::widgets::Cell::from(agent.status.to_string()).style(Style::default().fg(agent.status.color())),
                ratatui::widgets::Cell::from(uptime),
                ratatui::widgets::Cell::from(format!("{:.2}", agent.messages_per_second)),
                ratatui::widgets::Cell::from(agent.error_count.to_string()),
            ])
        });

        let table = Table::new(rows)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .widths(&[
                Constraint::Length(8),
                Constraint::Length(15),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(8),
            ]);

        frame.render_stateful_widget(table, area, &mut self.agent_table_state);
    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let log_items: Vec<ListItem> = self.logs.iter().rev().map(|log| {
            let content = Line::from(vec![
                Span::styled(
                    format!("[{}] ", log.timestamp.format("%H:%M:%S")),
                    Style::default().fg(Color::Gray)
                ),
                Span::styled(
                    format!("{} ", log.level.symbol()),
                    Style::default().fg(log.level.color())
                ),
                Span::raw(&log.message),
            ]);
            ListItem::new(content)
        }).collect();

        let logs_list = List::new(log_items)
            .block(Block::default().borders(Borders::ALL).title("Logs"))
            .highlight_style(Style::default().bg(Color::DarkGray));

        frame.render_stateful_widget(logs_list, area, &mut self.log_list_state);
    }

    fn render_performance(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // CPU and Memory info
        let perf_text = format!(
            "CPU Usage: {:.1}%\nMemory Usage: {} MB / {} MB ({:.1}%)\nUptime: {}",
            self.metrics.cpu_usage,
            self.metrics.memory_usage / 1024 / 1024,
            self.metrics.memory_total / 1024 / 1024,
            (self.metrics.memory_usage as f64 / self.metrics.memory_total as f64) * 100.0,
            format_duration(self.metrics.uptime)
        );
        let perf_info = Paragraph::new(perf_text)
            .block(Block::default().title("System Performance").borders(Borders::ALL));
        frame.render_widget(perf_info, chunks[0]);

        // Message rate info
        let msg_text = format!(
            "Total Messages: {}\nMessages/Second: {:.2}\nActive Agents: {}",
            self.metrics.total_messages,
            self.metrics.messages_per_second,
            self.metrics.active_agents
        );
        let msg_info = Paragraph::new(msg_text)
            .block(Block::default().title("Message Statistics").borders(Borders::ALL));
        frame.render_widget(msg_info, chunks[1]);
    }

    fn render_alerts(&mut self, frame: &mut Frame, area: Rect) {
        let alert_items: Vec<ListItem> = self.alerts.iter().map(|alert| {
            let content = Line::from(vec![
                Span::styled(
                    format!("[{}] ", alert.timestamp.format("%H:%M:%S")),
                    Style::default().fg(Color::Gray)
                ),
                Span::styled(
                    format!("{} ", alert.level.symbol()),
                    Style::default().fg(alert.level.color())
                ),
                Span::raw(&alert.message),
                if alert.acknowledged {
                    Span::styled(" [ACK]", Style::default().fg(Color::Green))
                } else {
                    Span::raw("")
                },
            ]);
            ListItem::new(content)
        }).collect();

        let alerts_list = List::new(alert_items)
            .block(Block::default().borders(Borders::ALL).title("Alerts"))
            .highlight_style(Style::default().bg(Color::DarkGray));

        frame.render_widget(alerts_list, area);
    }

    fn render_config(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(config) = &self.config {
            let editing_status = if self.config_editing {
                "🔧 EDITING MODE - Use arrow keys to navigate, Enter to edit values"
            } else {
                "👀 VIEW MODE"
            };

            let config_text = format!(
                "📝 Configuration Settings - {}\n\n\
                🎯 Streamers: {}\n\
                👥 Max Concurrent Agents: {}\n\
                🔄 Retry Attempts: {}\n\
                ⏱️  Delay Range: {} - {} ms\n\
                📊 API Port: {}\n\
                🌐 Dashboard Port: {}\n\
                📁 Output Format: {}\n\
                📂 Output Directory: {}\n\
                🔄 File Rotation Size: {}\n\
                ⏰ File Rotation Time: {}\n\
                🎭 Stealth Features:\n\
                  • User Agent Randomization: {}\n\
                  • Human Behavior Simulation: {}\n\
                  • Proxy Rotation: {}\n\
                  • Fingerprint Randomization: {}\n\n\
                💡 Press 'e' to {} configuration\n\
                💾 Press 's' to save changes (when editing)\n\
                🚫 Press 'Esc' to cancel editing",
                editing_status,
                config.streamers.join(", "),
                config.agents.max_concurrent,
                config.agents.retry_attempts,
                config.agents.delay_range.0,
                config.agents.delay_range.1,
                config.monitoring.api_port,
                config.monitoring.dashboard_port.unwrap_or(8888),
                config.output.format,
                config.output.directory.display(),
                config.output.rotation_size,
                config.output.rotation_time,
                if config.stealth.randomize_user_agents { "✅" } else { "❌" },
                if config.stealth.simulate_human_behavior { "✅" } else { "❌" },
                if config.stealth.proxy_rotation { "✅" } else { "❌" },
                if config.stealth.fingerprint_randomization { "✅" } else { "❌" },
                if self.config_editing { "exit edit mode for" } else { "edit" }
            );

            let title = if self.config_editing {
                "Configuration (EDITING)"
            } else {
                "Configuration"
            };

            let style = if self.config_editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let config_paragraph = Paragraph::new(config_text)
                .block(Block::default().title(title).borders(Borders::ALL))
                .style(style)
                .wrap(Wrap { trim: true });

            frame.render_widget(config_paragraph, area);
        } else {
            let no_config = Paragraph::new("⚠️  No configuration loaded\n\nConfiguration will be available once the system is fully initialized.")
                .block(Block::default().title("Configuration").borders(Borders::ALL))
                .style(Style::default().fg(Color::Yellow));

            frame.render_widget(no_config, area);
        }
    }
}

impl TUIMonitor for Dashboard {
    fn render(&mut self, frame: &mut Frame) -> Result<()> {
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(frame.size());

        // Render tabs
        let tab_titles = [
            Tab::Overview,
            Tab::Agents,
            Tab::Logs,
            Tab::Performance,
            Tab::Alerts,
            Tab::Config,
        ]
        .iter()
        .map(|t| t.title())
        .collect::<Vec<_>>();

        let tabs = Tabs::new(tab_titles)
            .block(Block::default().borders(Borders::ALL).title("Twitch Chat Scraper"))
            .select(self.current_tab as usize)
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Yellow));

        frame.render_widget(tabs, main_layout[0]);

        // Render current tab content
        match self.current_tab {
            Tab::Overview => self.render_overview(frame, main_layout[1]),
            Tab::Agents => self.render_agents(frame, main_layout[1]),
            Tab::Logs => self.render_logs(frame, main_layout[1]),
            Tab::Performance => self.render_performance(frame, main_layout[1]),
            Tab::Alerts => self.render_alerts(frame, main_layout[1]),
            Tab::Config => self.render_config(frame, main_layout[1]),
        }

        // Show help popup if requested
        if self.show_help {
            let area = centered_rect(60, 50, frame.size());
            frame.render_widget(Clear, area);
            let block = Block::default().title("Help").borders(Borders::ALL);
            frame.render_widget(block, area);
        }

        Ok(())
    }

    fn handle_input(&mut self, event: Event) -> Result<Action> {
        if let Event::Key(key) = event {
            if self.show_help {
                if matches!(key.code, KeyCode::Char('h') | KeyCode::Char('?') | KeyCode::Esc) {
                    self.show_help = false;
                }
                return Ok(Action::Continue);
            }

            match key.code {
                KeyCode::Char('q') => return Ok(Action::Quit),
                KeyCode::Char('h') | KeyCode::Char('?') => {
                    self.show_help = true;
                }
                KeyCode::Tab => {
                    self.current_tab = match self.current_tab {
                        Tab::Overview => Tab::Agents,
                        Tab::Agents => Tab::Logs,
                        Tab::Logs => Tab::Performance,
                        Tab::Performance => Tab::Alerts,
                        Tab::Alerts => Tab::Config,
                        Tab::Config => Tab::Overview,
                    };
                }
                KeyCode::Char('1') => self.current_tab = Tab::Overview,
                KeyCode::Char('2') => self.current_tab = Tab::Agents,
                KeyCode::Char('3') => self.current_tab = Tab::Logs,
                KeyCode::Char('4') => self.current_tab = Tab::Performance,
                KeyCode::Char('5') => self.current_tab = Tab::Alerts,
                KeyCode::Char('6') => self.current_tab = Tab::Config,
                KeyCode::Char('e') if self.current_tab == Tab::Config => {
                    self.config_editing = !self.config_editing;
                }
                KeyCode::Char('s') if self.current_tab == Tab::Config && self.config_editing => {
                    if let Some(ref config_manager) = self.config_manager {
                        if let Some(ref config) = self.config {
                            match config_manager.save_config(config).await {
                                Ok(_) => {
                                    self.add_alert(AlertLevel::Info, "Config Saved".to_string(), "Configuration saved successfully".to_string(), None);
                                }
                                Err(e) => {
                                    self.add_alert(AlertLevel::Critical, "Save Failed".to_string(), format!("Failed to save config: {}", e), None);
                                }
                            }
                        }
                    }
                    self.config_editing = false;
                }
                KeyCode::Esc if self.current_tab == Tab::Config && self.config_editing => {
                    self.config_editing = false;
                }
                KeyCode::Up => {
                    match self.current_tab {
                        Tab::Agents => {
                            let selected = self.agent_table_state.selected().unwrap_or(0);
                            if selected > 0 {
                                self.agent_table_state.select(Some(selected - 1));
                            }
                        }
                        Tab::Logs => {
                            let selected = self.log_list_state.selected().unwrap_or(0);
                            if selected > 0 {
                                self.log_list_state.select(Some(selected - 1));
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Down => {
                    match self.current_tab {
                        Tab::Agents => {
                            let selected = self.agent_table_state.selected().unwrap_or(0);
                            if selected < self.agents.len().saturating_sub(1) {
                                self.agent_table_state.select(Some(selected + 1));
                            }
                        }
                        Tab::Logs => {
                            let selected = self.log_list_state.selected().unwrap_or(0);
                            if selected < self.logs.len().saturating_sub(1) {
                                self.log_list_state.select(Some(selected + 1));
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Ok(Action::Continue)
    }

    fn update_metrics(&mut self, metrics: SystemMetrics) {
        self.metrics = metrics;
    }

    fn update_agents(&mut self, agents: Vec<AgentInfo>) {
        self.agents = agents;
        // Ensure selection is not out of bounds
        if let Some(selected) = self.agent_table_state.selected() {
            if selected >= self.agents.len() {
                self.agent_table_state.select(None);
            }
        }
    }


}

// Helper functions
fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m {}s", minutes, seconds)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}