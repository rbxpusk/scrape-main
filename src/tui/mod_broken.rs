use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Cell, Chart, Clear, Dataset, Gauge, GraphType, List, ListItem, ListState, Paragraph,
        Row, Table, TableState, Tabs, Wrap,
    },
    Frame,
};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use crate::agents::{AgentId, AgentStatus};

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
}

impl Tab {
    fn title(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Agents => "Agents",
            Tab::Logs => "Logs",
            Tab::Performance => "Performance",
            Tab::Alerts => "Alerts",
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
            AlertLevel::Critical => "🚨",
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
            LogLevel::Debug => Color::Gray,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            LogLevel::Info => "ℹ",
            LogLevel::Warning => "⚠",
            LogLevel::Error => "✗",
            LogLevel::Debug => "◦",
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
            border_color: Color::Gray,
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
    next_alert_id: u64,

    // UI state
    current_tab: Tab,
    agent_table_state: TableState,
    log_list_state: ListState,
    alert_table_state: TableState,
    show_help: bool,

    // Message rate tracking for real-time display
    message_history: Vec<(std::time::Instant, u64)>,
    last_message_count: u64,

    // Performance tracking for graphs
    performance_history: VecDeque<PerformanceData>,

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
                memory_total: 1, // Placeholder, should be updated
                uptime: std::time::Duration::from_secs(0),
            },
            agents: Vec::new(),
            logs: Vec::new(),
            alerts: Vec::new(),
            next_alert_id: 0,
            current_tab: Tab::Overview,
            agent_table_state: TableState::default(),
            log_list_state: ListState::default(),
            alert_table_state: TableState::default(),
            show_help: false,
            message_history: Vec::new(),
            last_message_count: 0,
            performance_history: VecDeque::with_capacity(300), // Store 5 minutes of data (300 seconds)
            theme: CustomTheme::default(),
            custom_css_path: None,
        }
    }

    pub fn with_custom_theme(mut self, css_path: Option<PathBuf>) -> Self {
        if let Some(path) = &css_path {
            if let Ok(theme) = Self::load_custom_theme(path) {
                self.theme = theme;
            }
        }
        self.custom_css_path = css_path;
        self
    }

    fn load_custom_theme(_path: &PathBuf) -> Result<CustomTheme> {
        // Placeholder for theme loading logic
        unimplemented!("Custom theme loading is not yet implemented.")
    }

    pub fn add_alert(&mut self, level: AlertLevel, title: String, message: String, agent_id: Option<AgentId>) {
        let alert_id = self.next_alert_id;
        self.next_alert_id += 1;
        self.alerts.push(Alert {
            id: alert_id,
            timestamp: chrono::Utc::now(),
            level,
            message: format!("{}: {}", title, message),
            agent_id,
            acknowledged: false,
        });
    }

    pub fn acknowledge_alert(&mut self, alert_id: u64) {
        if let Some(alert) = self.alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
        }
    }

    pub fn update_performance_history(&mut self) {
        if self.performance_history.len() == 300 {
            self.performance_history.pop_front();
        }
        self.performance_history.push_back(PerformanceData {
            timestamp: std::time::Instant::now(),
            cpu_usage: self.metrics.cpu_usage,
            memory_usage: self.metrics.memory_usage,
            messages_per_second: self.metrics.messages_per_second,
        });
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);
        if self.logs.len() > 1000 { // Limit log history
            self.logs.remove(0);
        }


    pub fn update_message_rate(&mut self) {
        let now = std::time::Instant::now();
        let current_message_count = self.metrics.total_messages;
        self.message_history.push((now, current_message_count));

        // Remove entries older than 5 seconds
        self.message_history.retain(|(t, _)| now.duration_since(*t).as_secs() < 5);

        if let Some((oldest_time, oldest_count)) = self.message_history.first() {
            let duration = now.duration_since(*oldest_time).as_secs_f64();
            if duration > 0.0 {
                self.metrics.messages_per_second = (current_message_count - oldest_count) as f64 / duration;
            }
        }
    }

    fn render_overview(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        self.render_system_metrics(frame, chunks[0]);

        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        self.render_agent_summary(frame, right_chunks[0]);
        self.render_recent_activity(frame, right_chunks[1]);
    }

    fn render_system_metrics(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("System Metrics").borders(Borders::ALL);
        frame.render_widget(block, area);

        let inner_area = Layout::default()
            .margin(1)
            .constraints([Constraint::Length(1); 7])
            .split(area)[0];

        let metrics = [
            ("Uptime:", format_duration(self.metrics.uptime)),
            ("Active Agents:", self.metrics.active_agents.to_string()),
            ("Total Messages:", self.metrics.total_messages.to_string()),
            ("Messages/sec:", format!("{:.2}", self.metrics.messages_per_second)),
            ("CPU Usage:", format!("{:.2}%", self.metrics.cpu_usage)),
            ("Memory Usage:", format!("{} / {} MB", self.metrics.memory_usage / 1024 / 1024, self.metrics.memory_total / 1024 / 1024)),
        ];

        for (i, (title, value)) in metrics.iter().enumerate() {
            let line = Line::from(vec![
                Span::styled(format!("{:<18}", title), Style::default().fg(self.theme.accent_color)),
                Span::raw(value.clone()),
            ]);
            frame.render_widget(Paragraph::new(line), Layout::default().constraints([Constraint::Length(1)]).split(inner_area)[i]);
        }
    }

    fn render_agent_summary(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("Agent Status Summary").borders(Borders::ALL);
        frame.render_widget(block, area);

        let mut status_counts = AGENT_STATUS_ORDER.iter().map(|s| (*s, 0)).collect::<std::collections::HashMap<_,_>>();
        for agent in &self.agents {
            *status_counts.entry(agent.status).or_insert(0) += 1;
        }

        let inner_area = Layout::default()
            .margin(1)
            .constraints([Constraint::Length(1); AGENT_STATUS_ORDER.len()])
            .split(area)[0];

        for (i, status) in AGENT_STATUS_ORDER.iter().enumerate() {
            let count = status_counts.get(status).unwrap_or(&0);
            let line = Line::from(vec![
                Span::styled(format!("{:<15}", format!("{:?}", status)), Style::default().fg(status.color())),
                Span::raw(count.to_string()),
            ]);
            frame.render_widget(Paragraph::new(line), Layout::default().constraints([Constraint::Length(1)]).split(inner_area)[i]);
        }
    }

    fn render_recent_activity(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("Recent Activity").borders(Borders::ALL);
        let log_items: Vec<ListItem> = self.logs.iter().rev().take(10).map(|log| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", log.level.symbol()), Style::default().fg(log.level.color())),
                Span::raw(log.message.clone()),
            ]))
        }).collect();
        let log_list = List::new(log_items).block(block);
        frame.render_widget(log_list, area);
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["ID", "Channel", "Status", "Uptime", "Msgs/s", "Errors"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(self.theme.accent_color)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows = self.agents.iter().map(|agent| {
            Row::new(vec![
                Cell::from(agent.id.to_string()),
                Cell::from(agent.channel.clone()),
                Cell::from(Span::styled(format!("{:?}", agent.status), Style::default().fg(agent.status.color()))),
                Cell::from(format_duration(agent.uptime)),
                Cell::from(format!("{:.2}", agent.messages_per_second)),
                Cell::from(agent.error_count.to_string()),
            ])
        });

        let table = Table::new(rows)
            .header(header)
            .block(Block::default().title("Agents").borders(Borders::ALL))
            .widths(&[
                Constraint::Length(5),
                Constraint::Length(20),
                Constraint::Length(15),
                Constraint::Length(15),
                Constraint::Length(10),
                Constraint::Length(10),
            ])
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(table, area, &mut self.agent_table_state);
    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let log_items: Vec<ListItem> = self.logs.iter().rev().map(|log| {
            let content = Line::from(vec![
                Span::styled(format!("{} ", log.timestamp.format("%H:%M:%S")), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", log.level.symbol()), Style::default().fg(log.level.color())),
                Span::raw(log.message.clone()),
            ]);
            ListItem::new(content)
        }).collect();

        let list = List::new(log_items)
            .block(Block::default().title("Logs").borders(Borders::ALL))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(list, area, &mut self.log_list_state);
    }

    fn render_performance(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(area);
        self.render_cpu_graph(frame, chunks[0]);
        self.render_memory_graph(frame, chunks[1]);
        self.render_message_rate_graph(frame, chunks[2]);
    }

    fn render_cpu_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history.iter().enumerate().map(|(i, d)| (i as f64, d.cpu_usage as f64)).collect();
        let datasets = vec![Dataset::default()
            .name("CPU %")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .data(&data)];
        let chart = Chart::new(datasets)
            .block(Block::default().title("CPU Usage (%)").borders(Borders::ALL))
            .x_axis(Axis::default().bounds([0.0, 300.0]))
            .y_axis(Axis::default().bounds([0.0, 100.0]).labels(vec![
                Span::raw("0"),
                Span::raw("50"),
                Span::raw("100"),
            ]));
        frame.render_widget(chart, area);
    }

    fn render_memory_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history.iter().enumerate().map(|(i, d)| (i as f64, d.memory_usage as f64 / 1024.0 / 1024.0)).collect();
        let mem_total_mb = self.metrics.memory_total as f64 / 1024.0 / 1024.0;
        let datasets = vec![Dataset::default()
            .name("Memory MB")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .data(&data)];
        let chart = Chart::new(datasets)
            .block(Block::default().title("Memory Usage (MB)").borders(Borders::ALL))
            .x_axis(Axis::default().bounds([0.0, 300.0]))
            .y_axis(Axis::default().bounds([0.0, mem_total_mb]).labels(vec![
                Span::raw("0"),
                Span::raw(format!("{:.0}", mem_total_mb / 2.0)),
                Span::raw(format!("{:.0}", mem_total_mb)),
            ]));
        frame.render_widget(chart, area);
    }

    fn render_message_rate_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history.iter().enumerate().map(|(i, d)| (i as f64, d.messages_per_second)).collect();
        let max_rate = data.iter().map(|(_, y)| *y).fold(0.0, f64::max).max(10.0); // Ensure a minimum scale
        let datasets = vec![Dataset::default()
            .name("Msgs/s")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .data(&data)];
        let chart = Chart::new(datasets)
            .block(Block::default().title("Message Rate (msgs/s)").borders(Borders::ALL))
            .x_axis(Axis::default().bounds([0.0, 300.0]))
            .y_axis(Axis::default().bounds([0.0, max_rate]).labels(vec![
                Span::raw("0"),
                Span::raw(format!("{:.1}", max_rate / 2.0)),
                Span::raw(format!("{:.1}", max_rate)),
            ]));
        frame.render_widget(chart, area);
    }

    fn render_alerts(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["ID", "Time", "Level", "Message"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(self.theme.accent_color)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows = self.alerts.iter().rev().map(|alert| {
            let style = if alert.acknowledged {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(alert.level.color())
            };
            Row::new(vec![
                Cell::from(alert.id.to_string()),
                Cell::from(alert.timestamp.format("%H:%M:%S").to_string()),
                Cell::from(Span::styled(format!("{:?}", alert.level), style)),
                Cell::from(alert.message.clone()),
            ]).style(style)
        });

        let table = Table::new(rows)
            .header(header)
            .block(Block::default().title("Alerts").borders(Borders::ALL))
            .widths(&[
                Constraint::Length(5),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Min(20),
            ])
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(table, area, &mut self.alert_table_state);
    }

    fn render_help_popup(&self, frame: &mut Frame) {
        let block = Block::default().title("Help").borders(Borders::ALL);
        let area = centered_rect(60, 50, frame.size());
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let text = vec![
            Line::from(vec![Span::styled("q", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Quit")]),
            Line::from(vec![Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Cycle tabs")]),
            Line::from(vec![Span::styled("h or ?", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Show/Hide help")]),
            Line::from(vec![Span::styled("Up/Down Arrow", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Navigate lists/tables")]),
            Line::from(vec![Span::styled("a", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Acknowledge alert (in Alerts tab)")]),
        ];

        let paragraph = Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        let inner_area = Layout::default().margin(1).split(area)[0];
        frame.render_widget(paragraph, inner_area);
    }

    fn handle_agents_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Up => {
                let selected = self.agent_table_state.selected().unwrap_or(0);
                if selected > 0 {
                    self.agent_table_state.select(Some(selected - 1));
                }
            }
            KeyCode::Down => {
                let selected = self.agent_table_state.selected().unwrap_or(0);
                let total_agents = self.agents.len();
                if total_agents > 0 && selected < total_agents - 1 {
                    self.agent_table_state.select(Some(selected + 1));
                }
            }
            KeyCode::Char('a') => {
                if let Some(selected_index) = self.agent_table_state.selected() {
                    if let Some(agent) = self.agents.get(selected_index) {
                        if let Some(alert_id) = agent.alert_id {
                            self.acknowledge_alert(alert_id);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(Action::Continue)
    }

    fn handle_logs_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Up => {
                let selected = self.log_list_state.selected().unwrap_or(0);
                if selected > 0 {
                    self.log_list_state.select(Some(selected - 1));
                }
            }
            KeyCode::Down => {
                let selected = self.log_list_state.selected().unwrap_or(0);
                let total_logs = self.logs.len();
                if total_logs > 0 && selected < total_logs - 1 {
                    self.log_list_state.select(Some(selected + 1));
                }
            }
            _ => {}
        }
        Ok(Action::Continue)
    }
}

impl TUIMonitor for Dashboard {
    fn render(&mut self, frame: &mut Frame) -> Result<()> {
        self.update_message_rate();
        self.update_performance_history();

        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(frame.size());

        let tab_titles: Vec<Line> = [Tab::Overview, Tab::Agents, Tab::Logs, Tab::Performance, Tab::Alerts]
            .iter()
            .map(|t| Line::from(t.title()))
            .collect();

        let tabs = Tabs::new(tab_titles)
            .block(Block::default().borders(Borders::ALL).title("Dashboard"))
            .select(self.current_tab as usize)
            .style(Style::default().fg(self.theme.text_color))
            .highlight_style(
                Style::default()
                    .fg(self.theme.accent_color)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(tabs, main_layout[0]);

        let inner_area = main_layout[1];

        match self.current_tab {
            Tab::Overview => self.render_overview(frame, inner_area),
            Tab::Agents => self.render_agents(frame, inner_area),
            Tab::Logs => self.render_logs(frame, inner_area),
            Tab::Performance => self.render_performance(frame, inner_area),
            Tab::Alerts => self.render_alerts(frame, inner_area),
        }

        if self.show_help {
            self.render_help_popup(frame);
        }

        Ok(())
    }

    fn handle_input(&mut self, event: Event) -> Result<Action> {
        if let Event::Key(key) = event {
            if self.show_help {
                self.show_help = false;
                return Ok(Action::Continue);
            }

            match key.code {
                KeyCode::Char('q') => return Ok(Action::Quit),
                KeyCode::Char('h') | KeyCode::Char('?') => self.show_help = true,
                KeyCode::Tab => {
                    let current_index = self.current_tab as usize;
                    let next_index = (current_index + 1) % 5; // 5 tabs
                    self.current_tab = match next_index {
                        0 => Tab::Overview,
                        1 => Tab::Agents,
                        2 => Tab::Logs,
                        3 => Tab::Performance,
                        4 => Tab::Alerts,
                        _ => unreachable!(),
                    };
                }
                _ => {
                    return match self.current_tab {
                        Tab::Agents => self.handle_agents_input(key),
                        Tab::Logs => self.handle_logs_input(key),
                        _ => Ok(Action::Continue), // No specific key handling for other tabs
                    };
                }
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

fn render_agent_summary(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Agent Summary");

    let agent_counts = HashMap::new();

    let summary_items: Vec<ListItem> = AGENT_STATUS_ORDER
        .iter()
        .map(|status| {
            let count = agent_counts.get(status).unwrap_or(&0);
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", status.symbol()),
                    Style::default().fg(status.color()),
                ),
                Span::raw(format!("{:<12}: ", status.to_string())),
                Span::styled(count.to_string(), Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(summary_items).block(block);
    frame.render_widget(list, area);
}

    fn render_recent_activity(&self, frame: &mut Frame, area: Rect) {
        let recent_logs: Vec<ListItem> = self.logs.iter().rev()
            .take(area.height.saturating_sub(2) as usize)
            .map(|log| {
                let time_str = log.timestamp.format("%H:%M:%S").to_string();
                let agent_str = if let Some(agent_id) = log.agent_id {
                    format!("[{}]", &agent_id.to_string()[..8])
                } else {
                    "[SYSTEM]".to_string()
                };
                
                ListItem::new(Line::from(vec![
                    Span::styled(time_str, Style::default().fg(Color::Gray)),
                    Span::raw(" "),
                    Span::styled(log.level.symbol(), Style::default().fg(log.level.color())),
                    Span::raw(" "),
                    Span::styled(agent_str, Style::default().fg(Color::Blue)),
                    Span::raw(" "),
                    Span::raw(&log.message),
                ]))
            })
            .collect();

        let activity_list = List::new(recent_logs)
            .block(Block::default().borders(Borders::ALL).title("Recent Activity"));

        frame.render_widget(activity_list, area);
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(80),
                Constraint::Percentage(20),
            ])
            .split(area);

        let header_cells = ["ID", "Status", "Uptime", "Msgs/Sec", "Errors"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
        
        let header = Row::new(header_cells)
            .style(Style::default().bg(Color::DarkGray))
            .height(1)
            .bottom_margin(1);

        let rows = self.agents.iter().map(|agent| {
            let cells = [
                Cell::from(agent.id.to_string()),
                Cell::from(Line::from(vec![
                    Span::styled(agent.status.symbol(), Style::default().fg(agent.status.color())),
                    Span::raw(" "),
                    Span::raw(agent.status.to_string()),
                ])),
                Cell::from(format!("{:?}", agent.uptime)),
                Cell::from(format!("{:.2}", agent.messages_per_second)),
                Cell::from(agent.error_count.to_string()),
            ];
            Row::new(cells).height(1)
        });

        let table = Table::new(rows, vec![Constraint::Percentage(20); 5])
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        frame.render_stateful_widget(table, chunks[0], &mut self.agent_table_state);

        // Render agent details in the second chunk
        if let Some(selected) = self.agent_table_state.selected() {
            if let Some(agent) = self.agents.get(selected) {
                let details_text = vec![
                    Line::from(vec![Span::raw("ID: "), Span::raw(agent.id.to_string())]),
                    Line::from(vec![Span::raw("Channel: "), Span::raw(&agent.channel)]),
                    Line::from(vec![Span::raw("Status: "), Span::raw(agent.status.to_string())]),
                ];
                let details_paragraph = Paragraph::new(details_text)
                    .block(Block::default().borders(Borders::ALL).title("Agent Details"));
                frame.render_widget(details_paragraph, chunks[1]);
            }
        }
    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let log_items: Vec<ListItem> = self.logs.iter().rev().map(|log| {
            let content = Line::from(vec![
                Span::styled(log.timestamp.format("%H:%M:%S").to_string(), Style::default().fg(Color::Gray)),
                Span::raw(" "),
                Span::styled(log.level.symbol(), Style::default().fg(log.level.color())),
                Span::raw(" "),
                Span::raw(&log.message),
            ]);
            ListItem::new(content)
        }).collect();

        let log_list = List::new(log_items)
            .block(Block::default().borders(Borders::ALL).title("Logs"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        frame.render_stateful_widget(log_list, area, &mut self.log_list_state);
    }

    fn render_performance(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(area);

        self.render_cpu_graph(frame, chunks[0]);
        self.render_memory_graph(frame, chunks[1]);
        self.render_message_rate_graph(frame, chunks[2]);
    }

    fn render_cpu_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self
            .performance_history
            .iter()
            .enumerate()
            .map(|(i, data)| (i as f64, data.cpu_usage as f64))
            .collect();

        let dataset = Dataset::default()
            .name("CPU Usage")
            .marker(symbols::Marker::Dot)
            .style(Style::default().fg(Color::Red))
            .data(&data);

        let x_axis = Axis::default()
            .title("Time")
            .style(Style::default().fg(Color::Gray))
            .bounds([0.0, data.len() as f64]);
        
        let y_axis = Axis::default()
            .title("Usage (%)")
            .style(Style::default().fg(Color::Gray))
            .bounds([0.0, 100.0])
            .labels(vec![
                Span::raw("0"),
                Span::raw("50"),
                Span::raw("100"),
            ]);

        let chart = Chart::new(vec![dataset])
            .block(Block::default().borders(Borders::ALL).title("CPU Usage"))
            .x_axis(x_axis)
            .y_axis(y_axis);

        frame.render_widget(chart, area);
    }

    fn render_memory_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history
            .iter()
            .enumerate()
            .map(|(i, data)| (i as f64, data.memory_usage as f64 / 1024.0 / 1024.0))
            .collect();

        let total_memory_mb = self.metrics.memory_total as f64 / 1024.0 / 1024.0;

        let dataset = Dataset::default()
            .name("Memory Usage")
            .marker(symbols::Marker::Dot)
            .style(Style::default().fg(Color::Yellow))
            .data(&data);

        let x_axis = Axis::default()
            .title("Time")
            .style(Style::default().fg(Color::Gray))
            .bounds([0.0, data.len() as f64]);

        let y_axis = Axis::default()
            .title("Usage (MB)")
            .style(Style::default().fg(Color::Gray))
            .bounds([0.0, total_memory_mb])
            .labels(vec![
                Span::raw("0"),
                Span::raw(format!("{:.0}", total_memory_mb / 2.0)),
                Span::raw(format!("{:.0}", total_memory_mb)),
            ]);

        let chart = Chart::new(vec![dataset])
            .block(Block::default().borders(Borders::ALL).title("Memory Usage"))
            .x_axis(x_axis)
            .y_axis(y_axis);

        frame.render_widget(chart, area);
    }

    fn render_message_rate_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history
            .iter()
            .enumerate()
            .map(|(i, data)| (i as f64, data.messages_per_second))
            .collect();

        let max_rate = data.iter().map(|(_, y)| *y).fold(0.0, f64::max).max(10.0);

        let dataset = Dataset::default()
            .name("Message Rate")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Cyan))
            .data(&data);

        let x_axis = Axis::default()
            .title("Time")
            .style(Style::default().fg(Color::Gray))
            .bounds([0.0, data.len() as f64]);

        let y_axis = Axis::default()
            .title("Msgs/sec")
            .style(Style::default().fg(Color::Gray))
            .bounds([0.0, max_rate])
            .labels(vec![
                Span::raw("0"),
                Span::raw(format!("{:.1}", max_rate / 2.0)),
                Span::raw(format!("{:.1}", max_rate)),
            ]);

        let chart = Chart::new(vec![dataset])
            .block(Block::default().borders(Borders::ALL).title("Message Rate"))
            .x_axis(x_axis)
            .y_axis(y_axis);

        frame.render_widget(chart, area);
    }

    fn render_alerts(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["ID", "Level", "Message", "Timestamp"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
        
        let header = Row::new(header_cells)
            .style(Style::default().bg(Color::DarkGray))
            .height(1)
            .bottom_margin(1);

        let rows = self.alerts.iter().map(|alert| {
            let cells = [
                Cell::from(alert.id.to_string()),
                Cell::from(Line::from(vec![
                    Span::styled(alert.level.symbol(), Style::default().fg(alert.level.color())),
                    Span::raw(" "),
                    Span::raw(format!("{:?}", alert.level)),
                ])),
                Cell::from(alert.message.clone()),
                Cell::from(alert.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
            ];
            Row::new(cells).height(1)
        });

        let table = Table::new(rows, vec![Constraint::Percentage(10), Constraint::Percentage(15), Constraint::Percentage(50), Constraint::Percentage(25)])
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Alerts"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        frame.render_stateful_widget(table, area, &mut self.alert_table_state);
    }

    fn render_help_popup(&self, frame: &mut Frame) {
        let area = centered_rect(60, 50, frame.size());
        let help_text = "
        Help - Key Bindings
        -------------------
        q: Quit
        ?: Toggle Help
        
        Tabs:
        1: Overview
        2: Agents
        3: Logs
        4: Performance
        5: Alerts
        
        Agents Tab:
        ↑/↓: Navigate agents
        
        Logs Tab:
        ↑/↓: Scroll logs
        ";
        let paragraph = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Help").title_alignment(Alignment::Center))
            .wrap(Wrap { trim: true });

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_agents_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Up => {
                if let Some(selected) = self.agent_table_state.selected() {
                    if selected > 0 {
                        self.agent_table_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down => {
                if let Some(selected) = self.agent_table_state.selected() {
                    if selected < self.agents.len() - 1 {
                        self.agent_table_state.select(Some(selected + 1));
                    }
                }
            }
            _ => {}
        }
        Ok(Action::Continue)
    }

    fn handle_logs_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Up => {
                if let Some(selected) = self.log_list_state.selected() {
                    if selected > 0 {
                        self.log_list_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down => {
                if let Some(selected) = self.log_list_state.selected() {
                    if selected < self.logs.len() - 1 {
                        self.log_list_state.select(Some(selected + 1));
                    }
                }
            }
            _ => {}
        }
        Ok(Action::Continue)
    }
}

impl TUIMonitor for Dashboard {
    fn render(&mut self, frame: &mut Frame) -> Result<()> {
        // Update message rate calculation
        self.update_message_rate();

        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(frame.size());

        let tab_titles = [
            Tab::Overview,
            Tab::Agents,
            Tab::Logs,
            Tab::Performance,
            Tab::Alerts,
        ]
        .iter()
        .map(|t| t.title())
        .collect();

        let tabs = Tabs::new(tab_titles)
            .block(Block::default().borders(Borders::ALL).title("Dashboard"))
            .select(self.current_tab as usize)
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

        frame.render_widget(tabs, main_layout[0]);

        match self.current_tab {
            Tab::Overview => self.render_overview(frame, main_layout[1]),
            Tab::Agents => self.render_agents(frame, main_layout[1]),
            Tab::Logs => self.render_logs(frame, main_layout[1]),
            Tab::Performance => self.render_performance(frame, main_layout[1]),
            Tab::Alerts => self.render_alerts(frame, main_layout[1]),
        }

        if self.show_help {
            self.render_help_popup(frame);
        }

        Ok(())
    }

    fn handle_input(&mut self, event: Event) -> Result<Action> {
        if let Event::Key(key_event) = event {
            if self.show_help {
                if key_event.code == KeyCode::Char('q') || key_event.code == KeyCode::Char('?') || key_event.code == KeyCode::Esc {
                    self.show_help = false;
                }
                return Ok(Action::Continue);
            }

            match key_event.code {
                KeyCode::Char('q') => return Ok(Action::Quit),
                KeyCode::Char('?') => self.show_help = true,
                KeyCode::Char('1') => self.current_tab = Tab::Overview,
                KeyCode::Char('2') => self.current_tab = Tab::Agents,
                KeyCode::Char('3') => self.current_tab = Tab::Logs,
                KeyCode::Char('4') => self.current_tab = Tab::Performance,
                KeyCode::Char('5') => self.current_tab = Tab::Alerts,
                _ => {
                    match self.current_tab {
                        Tab::Agents => return self.handle_agents_input(key_event),
                        Tab::Logs => return self.handle_logs_input(key_event),
                        _ => {}
                    }
                }
            }
        }
        Ok(Action::Continue)
    }
}

pub fn with_custom_theme(mut self, css_path: Option<PathBuf>) -> Self {
    if let Some(path) = &css_path {
        if let Ok(theme) = Self::load_custom_theme(path) {
            self.theme = theme;
        }
    }
    self.custom_css_path = css_path;
    self
}

fn load_custom_theme(_path: &PathBuf) -> Result<CustomTheme> {
    // Placeholder for theme loading logic
    unimplemented!("Custom theme loading is not yet implemented.")
}

pub fn add_alert(&mut self, level: AlertLevel, title: String, message: String, agent_id: Option<AgentId>) {
    let new_alert = Alert {
        id: self.alerts.len() as u64 + 1,
        timestamp: chrono::Utc::now(),
        level,
        title,
        message,
        agent_id,
        acknowledged: false,
    };
    self.alerts.push(new_alert);
}

pub fn acknowledge_alert(&mut self, alert_id: u64) {
    if let Some(alert) = self.alerts.iter_mut().find(|a| a.id == alert_id) {
        alert.acknowledged = true;
    }
}

fn update_performance_history(&mut self) {
    let now = std::time::Instant::now();
    let new_data = PerformanceData {
        timestamp: now,
        cpu_usage: self.metrics.cpu_usage,
        memory_usage: self.metrics.memory_usage,
        messages_per_second: self.metrics.messages_per_second,
        active_agents: self.metrics.active_agents,
        error_count: self.alerts.iter().filter(|a| a.level == AlertLevel::Critical).count() as u32,
    };
    self.performance_history.push_back(new_data);
    if self.performance_history.len() > 300 {
        self.performance_history.pop_front();
    }
}

pub fn add_log(&mut self, entry: LogEntry) {
    self.logs.push(entry);
    if self.logs.len() > 1000 { // Cap logs
        self.logs.remove(0);
    }
}

fn update_message_rate(&mut self) {
    let now = std::time::Instant::now();
    let total_messages = self.metrics.total_messages;

    self.message_history.push((now, total_messages));
    self.message_history.retain(|(t, _)| now.duration_since(*t).as_secs() < 60);

    if let Some((oldest_time, oldest_count)) = self.message_history.first() {
        let duration = now.duration_since(*oldest_time).as_secs_f64();
        if duration > 1.0 {
            self.metrics.messages_per_second = (total_messages - oldest_count) as f64 / duration;
        } else {
            self.metrics.messages_per_second = 0.0;
        }
    } else {
        self.metrics.messages_per_second = 0.0;
    }
    self.last_message_count = total_messages;
}

fn render_overview(&mut self, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    self.render_system_metrics(frame, chunks[0]);
    self.render_agent_summary(frame, chunks[1]);
    self.render_recent_activity(frame, chunks[2]);
}

fn render_system_metrics(&self, frame: &mut Frame, area: Rect) {
    let block = Block::default().title("System Metrics").borders(Borders::ALL);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .margin(1)
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let uptime_str = format_duration(self.metrics.uptime);
    let metrics_text = vec![
        Line::from(vec![
            Span::styled("Uptime: ", Style::default().fg(self.theme.primary_color)),
            Span::raw(uptime_str),
        ]),
        Line::from(vec![
            Span::styled("Active Agents: ", Style::default().fg(self.theme.primary_color)),
            Span::raw(self.metrics.active_agents.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Total Messages: ", Style::default().fg(self.theme.primary_color)),
            Span::raw(self.metrics.total_messages.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Messages/sec: ", Style::default().fg(self.theme.primary_color)),
            Span::raw(format!("{:.2}", self.metrics.messages_per_second)),
        ]),
    ];

    let metrics_paragraph = Paragraph::new(metrics_text);
    frame.render_widget(metrics_paragraph, chunks[0]);

    let cpu_gauge = Gauge::default()
        .block(Block::default().title("CPU Usage"))
        .gauge_style(Style::default().fg(self.theme.accent_color))
        .percent(self.metrics.cpu_usage as u16);
    frame.render_widget(cpu_gauge, chunks[1]);

    let mem_percent = if self.metrics.memory_total > 0 {
        (self.metrics.memory_usage * 100 / self.metrics.memory_total) as u16
    } else {
        0
    };
    let mem_gauge = Gauge::default()
        .block(Block::default().title("Memory Usage"))
        .gauge_style(Style::default().fg(self.theme.accent_color))
        .percent(mem_percent)
        .label(format!("{} / {} MB", self.metrics.memory_usage / 1024 / 1024, self.metrics.memory_total / 1024 / 1024));
    frame.render_widget(mem_gauge, chunks[2]);
}

fn render_agent_summary(&self, frame: &mut Frame, area: Rect) {
    let running_count = self.agents.iter().filter(|a| a.status == AgentStatus::Running).count();
    let errored_count = self.agents.iter().filter(|a| matches!(a.status, AgentStatus::Error(_))).count();
    let idle_count = self.agents.iter().filter(|a| a.status == AgentStatus::Idle).count();

    let summary_text = vec![
        Line::from(vec![
            Span::raw("Running: "),
            Span::styled(running_count.to_string(), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::raw("Errored: "),
            Span::styled(errored_count.to_string(), Style::default().fg(Color::Red)),
        ]),
        Line::from(vec![
            Span::raw("Idle: "),
            Span::styled(idle_count.to_string(), Style::default().fg(Color::Gray)),
        ]),
    ];

    let summary_paragraph = Paragraph::new(summary_text)
        .block(Block::default().title("Agent Summary").borders(Borders::ALL));

    frame.render_widget(summary_paragraph, area);
}

fn render_recent_activity(&self, frame: &mut Frame, area: Rect) {
    let recent_logs: Vec<ListItem> = self.logs.iter().rev()
        .take(area.height.saturating_sub(2) as usize)
        .map(|log| {
            let time_str = log.timestamp.format("%H:%M:%S").to_string();
            let agent_str = if let Some(agent_id) = log.agent_id {
                format!("[{}]", &agent_id.to_string()[..8])
            } else {
                "[SYSTEM]".to_string()
            };

            ListItem::new(Line::from(vec![
                Span::styled(time_str, Style::default().fg(Color::Gray)),
                Span::raw(" "),
                Span::styled(log.level.symbol(), Style::default().fg(log.level.color())),
                Span::raw(" "),
                Span::styled(agent_str, Style::default().fg(Color::Blue)),
                Span::raw(" "),
                Span::raw(&log.message),
            ]))
        })
        .collect();

    let activity_list = List::new(recent_logs)
        .block(Block::default().borders(Borders::ALL).title("Recent Activity"));

    frame.render_widget(activity_list, area);
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


    fn load_custom_theme(_path: &PathBuf) -> Result<CustomTheme> {
        // Placeholder for theme loading logic
        unimplemented!("Custom theme loading is not yet implemented.")
    }

    pub fn add_alert(&mut self, level: AlertLevel, title: String, message: String, agent_id: Option<AgentId>) {
        let new_alert = Alert {
            id: self.alerts.len() as u64 + 1,
            timestamp: chrono::Utc::now(),
            level,
            title,
            message,
            agent_id,
            acknowledged: false,
        };
        self.alerts.push(new_alert);
    }

    pub fn acknowledge_alert(&mut self, alert_id: u64) {
        if let Some(alert) = self.alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
        }
    }

    fn update_performance_history(&mut self) {
        let now = std::time::Instant::now();
        let new_data = PerformanceData {
            timestamp: now,
            cpu_usage: self.metrics.cpu_usage,
            memory_usage: self.metrics.memory_usage,
            messages_per_second: self.metrics.messages_per_second,
            active_agents: self.metrics.active_agents,
            error_count: self.alerts.iter().filter(|a| a.level == AlertLevel::Critical).count() as u32,
        };
        self.performance_history.push_back(new_data);
        if self.performance_history.len() > 300 {
            self.performance_history.pop_front();
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);
        if self.logs.len() > 1000 { // Cap logs
            self.logs.remove(0);
        }
    }

    fn update_message_rate(&mut self) {
        let now = std::time::Instant::now();
        let total_messages = self.metrics.total_messages;

        self.message_history.push((now, total_messages));
        self.message_history.retain(|(t, _)| now.duration_since(*t).as_secs() < 60);

        if let Some((oldest_time, oldest_count)) = self.message_history.first() {
            let duration = now.duration_since(*oldest_time).as_secs_f64();
            if duration > 1.0 {
                self.metrics.messages_per_second = (total_messages - oldest_count) as f64 / duration;
            } else {
                self.metrics.messages_per_second = 0.0;
            }
        } else {
            self.metrics.messages_per_second = 0.0;
        }
        self.last_message_count = total_messages;
    }

    fn render_overview(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(area);

        self.render_system_metrics(frame, chunks[0]);
        self.render_agent_summary(frame, chunks[1]);
        self.render_recent_activity(frame, chunks[2]);
    }

    fn render_system_metrics(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("System Metrics").borders(Borders::ALL);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .margin(1)
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(area);

        let uptime_str = format_duration(self.metrics.uptime);
        let metrics_text = vec![
            Line::from(vec![
                Span::styled("Uptime: ", Style::default().fg(self.theme.primary_color)),
                Span::raw(uptime_str),
            ]),
            Line::from(vec![
                Span::styled("Active Agents: ", Style::default().fg(self.theme.primary_color)),
                Span::raw(self.metrics.active_agents.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Total Messages: ", Style::default().fg(self.theme.primary_color)),
                Span::raw(self.metrics.total_messages.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Messages/sec: ", Style::default().fg(self.theme.primary_color)),
                Span::raw(format!("{:.2}", self.metrics.messages_per_second)),
            ]),
        ];

        let metrics_paragraph = Paragraph::new(metrics_text);
        frame.render_widget(metrics_paragraph, chunks[0]);

        let cpu_gauge = Gauge::default()
            .block(Block::default().title("CPU Usage"))
            .gauge_style(Style::default().fg(self.theme.accent_color))
            .percent(self.metrics.cpu_usage as u16);
        frame.render_widget(cpu_gauge, chunks[1]);

        let mem_percent = if self.metrics.memory_total > 0 {
            (self.metrics.memory_usage * 100 / self.metrics.memory_total) as u16
        } else {
            0
        };
        let mem_gauge = Gauge::default()
            .block(Block::default().title("Memory Usage"))
            .gauge_style(Style::default().fg(self.theme.accent_color))
            .percent(mem_percent)
            .label(format!("{} / {} MB", self.metrics.memory_usage / 1024 / 1024, self.metrics.memory_total / 1024 / 1024));
        frame.render_widget(mem_gauge, chunks[2]);
    }

    fn render_agent_summary(&self, frame: &mut Frame, area: Rect) {
        let running_count = self.agents.iter().filter(|a| a.status == AgentStatus::Running).count();
        let errored_count = self.agents.iter().filter(|a| matches!(a.status, AgentStatus::Error(_))).count();
        let idle_count = self.agents.iter().filter(|a| a.status == AgentStatus::Idle).count();

        let summary_text = vec![
            Line::from(vec![
                Span::raw("Running: "),
                Span::styled(running_count.to_string(), Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::raw("Errored: "),
                Span::styled(errored_count.to_string(), Style::default().fg(Color::Red)),
            ]),
            Line::from(vec![
                Span::raw("Idle: "),
                Span::styled(idle_count.to_string(), Style::default().fg(Color::Gray)),
            ]),
        ];

        let summary_paragraph = Paragraph::new(summary_text)
            .block(Block::default().title("Agent Summary").borders(Borders::ALL));

        frame.render_widget(summary_paragraph, area);
    }

    fn render_recent_activity(&self, frame: &mut Frame, area: Rect) {
        let recent_logs: Vec<ListItem> = self.logs.iter().rev()
            .take(area.height.saturating_sub(2) as usize)
            .map(|log| {
                let time_str = log.timestamp.format("%H:%M:%S").to_string();
                let agent_str = if let Some(agent_id) = log.agent_id {
                    format!("[{}]", &agent_id.to_string()[..8])
                } else {
                    "[SYSTEM]".to_string()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(time_str, Style::default().fg(Color::Gray)),
                    Span::raw(" "),
                    Span::styled(log.level.symbol(), Style::default().fg(log.level.color())),
                    Span::raw(" "),
                    Span::styled(agent_str, Style::default().fg(Color::Blue)),
                    Span::raw(" "),
                    Span::raw(&log.message),
                ]))
            })
            .collect();

        let activity_list = List::new(recent_logs)
            .block(Block::default().borders(Borders::ALL).title("Recent Activity"));

        frame.render_widget(activity_list, area);
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["ID", "Streamer", "Status", "Uptime", "Msgs", "Errors"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(self.theme.primary_color)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows: Vec<Row> = self.agents.iter().map(|agent| {
            let id_string = agent.id.to_string();
            let id_short = id_string[..8].to_string();
            let status_style = match agent.status {
                AgentStatus::Running => Style::default().fg(Color::Green),
                AgentStatus::Error(_) => Style::default().fg(Color::Red),
                AgentStatus::Starting => Style::default().fg(Color::Yellow),
                AgentStatus::Stopping => Style::default().fg(Color::Yellow),
                AgentStatus::Stopped => Style::default().fg(Color::Gray),
                AgentStatus::Idle => Style::default().fg(Color::Blue),
            };

            let status_text = match &agent.status {
                AgentStatus::Error(e) => e.clone(),
                s => format!("{:?}", s),
            };

            let cells = vec![
                Cell::from(id_short),
                Cell::from(agent.streamer.clone()),
                Cell::from(status_text).style(status_style),
                Cell::from(format_duration(agent.metrics.uptime)),
                Cell::from(agent.metrics.messages_scraped.to_string()),
                Cell::from(agent.metrics.error_count.to_string()),
            ];
            Row::new(cells)
        }).collect();

        let table = Table::new(rows)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .widths(&[
                Constraint::Length(10),
                Constraint::Length(20),
                Constraint::Length(15),
                Constraint::Length(15),
                Constraint::Length(10),
                Constraint::Length(10),
            ]);

        frame.render_stateful_widget(table, area, &mut self.agent_table_state);
    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let log_items: Vec<ListItem> = self.logs.iter().rev()
            .map(|log| {
                let time_str = log.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                let agent_str = log.agent_id.map_or_else(|| "[SYSTEM]".to_string(), |id| format!("[{}]
", &id.to_string()[..8]));
                let line = Line::from(vec![
                    Span::styled(time_str, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(log.level.symbol(), Style::default().fg(log.level.color())),
                    Span::raw(" "),
                    Span::styled(agent_str, Style::default().fg(Color::Blue)),
                    Span::raw(" "),
                    Span::raw(&log.message),
                ]);
                ListItem::new(line)
            })
            .collect();

        let log_list = List::new(log_items)
            .block(Block::default().borders(Borders::ALL).title("Logs"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(log_list, area, &mut self.log_list_state);
    }

    fn render_performance(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        let graph_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(chunks[0]);

        self.render_cpu_graph(frame, graph_chunks[0]);
        self.render_memory_graph(frame, graph_chunks[1]);
        self.render_message_rate_graph(frame, graph_chunks[2]);
        self.render_performance_metrics(frame, chunks[1]);
    }

    fn render_cpu_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history.iter().enumerate()
            .map(|(i, d)| (i as f64, d.cpu_usage as f64))
            .collect();

        let datasets = vec![Dataset::default()
            .name("CPU Usage (%)")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .graph_type(GraphType::Line)
            .data(&data)];

        let chart = Chart::new(datasets)
            .block(Block::default().title("CPU Usage").borders(Borders::ALL))
            .x_axis(
                Axis::default()
                    .title("Time (s)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 300.0]),
            )
            .y_axis(
                Axis::default()
                    .title("Usage (%)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 100.0])
                    .labels(vec![
                        Span::raw("0"),
                        Span::raw("50"),
                        Span::raw("100"),
                    ]),
            );

        frame.render_widget(chart, area);
    }

    fn render_memory_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history.iter().enumerate()
            .map(|(i, d)| (i as f64, d.memory_usage as f64 / 1024.0 / 1024.0))
            .collect();

        let max_mem = if self.metrics.memory_total > 0 {
            self.metrics.memory_total as f64 / 1024.0 / 1024.0
        } else {
            data.iter().map(|(_, y)| *y).fold(0.0, f64::max)
        };

        let datasets = vec![Dataset::default()
            .name("Memory Usage (MB)")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .graph_type(GraphType::Line)
            .data(&data)];

        let chart = Chart::new(datasets)
            .block(Block::default().title("Memory Usage").borders(Borders::ALL))
            .x_axis(
                Axis::default()
                    .title("Time (s)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 300.0]),
            )
            .y_axis(
                Axis::default()
                    .title("Usage (MB)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, max_mem])
                    .labels(vec![
                        Span::raw("0"),
                        Span::raw(format!("{:.0}", max_mem / 2.0)),
                        Span::raw(format!("{:.0}", max_mem)),
                    ]),
            );

        frame.render_widget(chart, area);
    }

    fn render_message_rate_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self.performance_history.iter().enumerate()
            .map(|(i, d)| (i as f64, d.messages_per_second))
            .collect();

        let max_rate = data.iter().map(|(_, y)| *y).fold(0.0, f64::max).max(10.0);

        let datasets = vec![Dataset::default()
            .name("Messages/sec")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .graph_type(GraphType::Line)
            .data(&data)];

        let chart = Chart::new(datasets)
            .block(Block::default().title("Message Rate").borders(Borders::ALL))
            .x_axis(
                Axis::default()
                    .title("Time (s)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 300.0]),
            )
            .y_axis(
                Axis::default()
                    .title("Rate")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, max_rate])
                    .labels(vec![
                        Span::raw("0"),
                        Span::raw(format!("{:.1}", max_rate / 2.0)),
                        Span::raw(format!("{:.1}", max_rate)),
                    ]),
            );

        frame.render_widget(chart, area);
    }

    fn render_performance_metrics(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("Performance Details").borders(Borders::ALL);
        frame.render_widget(block, area);

        if let Some(latest) = self.performance_history.back() {
            let text = vec![
                Line::from(vec![
                    Span::styled("CPU: ", Style::default().fg(self.theme.primary_color)),
                    Span::raw(format!("{:.2} %", latest.cpu_usage)),
                ]),
                Line::from(vec![
                    Span::styled("Memory: ", Style::default().fg(self.theme.primary_color)),
                    Span::raw(format!("{:.2} MB", latest.memory_usage as f64 / 1024.0 / 1024.0)),
                ]),
                Line::from(vec![
                    Span::styled("Msg/sec: ", Style::default().fg(self.theme.primary_color)),
                    Span::raw(format!("{:.2}", latest.messages_per_second)),
                ]),
                Line::from(vec![
                    Span::styled("Active Agents: ", Style::default().fg(self.theme.primary_color)),
                    Span::raw(latest.active_agents.to_string()),
                ]),
                Line::from(vec![
                    Span::styled("Critical Alerts: ", Style::default().fg(self.theme.primary_color)),
                    Span::raw(latest.error_count.to_string()),
                ]),
            ];

            let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area.inner(&ratatui::layout::Margin { vertical: 1, horizontal: 1 }));
        }
    }

    fn render_alerts(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["Time", "Level", "Agent", "Title", "Message"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(self.theme.primary_color)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows: Vec<Row> = self.alerts.iter().rev().map(|alert| {
            let time_str = alert.timestamp.format("%H:%M:%S").to_string();
            let level_style = Style::default().fg(alert.level.color());
            let agent_id_str = alert.agent_id.map_or_else(|| "N/A".to_string(), |id| id.to_string());

            let cells = vec![
                Cell::from(time_str),
                Cell::from(alert.level.symbol()).style(level_style),
                Cell::from(agent_id_str),
                Cell::from(alert.title.clone()),
                Cell::from(alert.message.clone()),
            ];
            Row::new(cells)
        }).collect();

        let table = Table::new(rows)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Alerts"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .widths(&[
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(20),
                Constraint::Min(30),
            ]);

        frame.render_stateful_widget(table, area, &mut self.alert_table_state);
    }

    fn render_help_popup(&self, frame: &mut Frame) {
        let area = centered_rect(60, 50, frame.size());
        let block = Block::default().title("Help").borders(Borders::ALL);
        let text = vec![
            Line::from(Span::styled("Key Bindings", Style::default().add_modifier(Modifier::BOLD))),
            Line::from("q: Quit"),
            Line::from("h: Show/Hide Help"),
            Line::from("Tab: Switch tabs"),
            Line::from("Up/Down: Navigate lists/tables"),
            Line::from("--- Agents Tab ---"),
            Line::from("s: Start new agent"),
            Line::from("x: Stop selected agent"),
            Line::from("r: Restart selected agent"),
            Line::from("--- Alerts Tab ---"),
            Line::from("a: Acknowledge selected alert"),
        ];

        let paragraph = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_agents_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Char('s') => Ok(Action::StartAgent("".to_string())), // Placeholder
            KeyCode::Char('x') => {
                if let Some(selected) = self.agent_table_state.selected() {
                    if let Some(agent) = self.agents.get(selected) {
                        return Ok(Action::StopAgent(agent.id));
                    }
                }
                Ok(Action::Continue)
            }
            KeyCode::Char('r') => {
                if let Some(selected) = self.agent_table_state.selected() {
                    if let Some(agent) = self.agents.get(selected) {
                        return Ok(Action::RestartAgent(agent.id));
                    }
                }
                Ok(Action::Continue)
            }
            KeyCode::Down => {
                let i = match self.agent_table_state.selected() {
                    Some(i) => if i >= self.agents.len() - 1 { 0 } else { i + 1 },
                    None => 0,
                };
                self.agent_table_state.select(Some(i));
                Ok(Action::Continue)
            }
            KeyCode::Up => {
                let i = match self.agent_table_state.selected() {
                    Some(i) => if i == 0 { self.agents.len() - 1 } else { i - 1 },
                    None => 0,
                };
                self.agent_table_state.select(Some(i));
                Ok(Action::Continue)
            }
            _ => Ok(Action::Continue),
        }
    }

    fn handle_logs_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Down => {
                let i = match self.log_list_state.selected() {
                    Some(i) => if i >= self.logs.len() - 1 { 0 } else { i + 1 },
                    None => 0,
                };
                self.log_list_state.select(Some(i));
                Ok(Action::Continue)
            }
            KeyCode::Up => {
                let i = match self.log_list_state.selected() {
                    Some(i) => if i == 0 { self.logs.len() - 1 } else { i - 1 },
                    None => 0,
                };
                self.log_list_state.select(Some(i));
                Ok(Action::Continue)
            }
            _ => Ok(Action::Continue),
        }
    }
    pub fn new() -> Self {
        let mut agent_table_state = TableState::default();
        agent_table_state.select(Some(0));

        let mut log_list_state = ListState::default();
        log_list_state.select(Some(0));

        let mut alert_table_state = TableState::default();
        alert_table_state.select(Some(0));

        let default_theme = CustomTheme {
            primary_color: Color::Cyan,
            secondary_color: Color::Blue,
            accent_color: Color::Yellow,
            error_color: Color::Red,
            warning_color: Color::Yellow,
            success_color: Color::Green,
            background_color: Color::Black,
            text_color: Color::White,
        };

        Self {
            metrics: SystemMetrics {
                active_agents: 0,
                total_messages: 0,
                messages_per_second: 0.0,
                cpu_usage: 0.0,
                memory_usage: 0,
                memory_total: 1,
                uptime: std::time::Duration::from_secs(0),
            },
            agents: Vec::new(),
            logs: Vec::new(),
            alerts: Vec::new(),
            current_tab: Tab::Overview,

            agent_table_state,
            log_list_state,
            alert_table_state,
            show_help: false,
            message_history: Vec::new(),
            last_message_count: 0,
            performance_history: VecDeque::with_capacity(300), // 5 minutes at 1 sample/second
            theme: default_theme,
            custom_css_path: None,
            next_alert_id: 1,
        }
    }

    pub fn with_custom_theme(mut self, css_path: Option<PathBuf>) -> Self {
        self.custom_css_path = css_path.clone();
        if let Some(path) = &css_path {
            if let Ok(theme) = Self::load_custom_theme(path) {
                self.theme = theme;
            }
        }
        self
    }

    fn load_custom_theme(_path: &PathBuf) -> Result<CustomTheme> {
        // Simple CSS-like theme loading (simplified implementation)
        // In a real implementation, this would parse CSS or a theme file
        Ok(CustomTheme {
            primary_color: Color::Magenta,
            secondary_color: Color::Cyan,
            accent_color: Color::Green,
            error_color: Color::Red,
            warning_color: Color::Yellow,
            success_color: Color::Green,
            background_color: Color::Black,
            text_color: Color::White,
        })
    }

    pub fn add_alert(&mut self, level: AlertLevel, title: String, message: String, agent_id: Option<AgentId>) {
        let alert = Alert {
            id: self.next_alert_id,
            timestamp: chrono::Utc::now(),
            level,
            title,
            message,
            agent_id,
            acknowledged: false,
        };

        self.alerts.push(alert);
        self.next_alert_id += 1;

        // Keep only the last 100 alerts
        if self.alerts.len() > 100 {
            self.alerts.remove(0);
        }

        // Update alert list selection to show the latest entry
        if !self.alerts.is_empty() {
            self.alert_table_state.select(Some(self.alerts.len() - 1));
        }
    }

    pub fn acknowledge_alert(&mut self, alert_id: u64) {
        if let Some(alert) = self.alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
        }
    }

    fn update_performance_history(&mut self) {
        let now = std::time::Instant::now();
        let total_errors = self.agents.iter().map(|a| a.metrics.error_count).sum();

        let perf_data = PerformanceData {
            timestamp: now,
            cpu_usage: self.metrics.cpu_usage,
            memory_usage: self.metrics.memory_usage,
            messages_per_second: self.metrics.messages_per_second,
            active_agents: self.metrics.active_agents,
            error_count: total_errors,
        };

        self.performance_history.push_back(perf_data);

        // Keep only the last 5 minutes of data (300 samples at 1/second)
        while self.performance_history.len() > 300 {
            self.performance_history.pop_front();
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);
        // Keep only the last 1000 log entries
        if self.logs.len() > 1000 {
            self.logs.remove(0);
        }

        // Update log list selection to show the latest entry
        if !self.logs.is_empty() {
            self.log_list_state.select(Some(self.logs.len() - 1));
        }
    }

    fn update_message_rate(&mut self) {
        let now = std::time::Instant::now();
        let current_count = self.metrics.total_messages;

        // Add current data point
        self.message_history.push((now, current_count));

        // Remove data points older than 60 seconds
        self.message_history.retain(|(timestamp, _)| {
            now.duration_since(*timestamp).as_secs() < 60
        });

        // Calculate messages per second over the last 10 seconds
        if let Some((oldest_time, oldest_count)) = self.message_history.first() {
            let time_diff = now.duration_since(*oldest_time).as_secs_f64();
            if time_diff > 0.0 {
                let message_diff = current_count.saturating_sub(*oldest_count);
                self.metrics.messages_per_second = message_diff as f64 / time_diff;
            }
        }

        self.last_message_count = current_count;
    }

    fn render_overview(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),  // System metrics
                Constraint::Length(8),  // Agent summary
                Constraint::Min(0),     // Recent activity
            ])
            .split(area);

        // System metrics panel
        self.render_system_metrics(frame, chunks[0]);

        // Agent summary panel
        self.render_agent_summary(frame, chunks[1]);

        // Recent activity panel
        self.render_recent_activity(frame, chunks[2]);
    }

    fn render_system_metrics(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Left side - Basic metrics
        let memory_percent = (self.metrics.memory_usage as f64 / self.metrics.memory_total as f64) * 100.0;
        let uptime_str = format_duration(self.metrics.uptime);

        let metrics_text = vec![
            Line::from(vec![
                Span::styled("Active Agents: ", Style::default()),
                Span::styled(
                    format!("{}", self.metrics.active_agents),
                    Style::default().fg(if self.metrics.active_agents > 0 { Color::Green } else { Color::Red }),
                ),
            ]),
            Line::from(vec![
                Span::styled("Total Messages: ", Style::default()),
                Span::styled(
                    format!("{}", self.metrics.total_messages),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("Messages/sec: ", Style::default()),
                Span::styled(
                    format!("{:.1}", self.metrics.messages_per_second),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::styled("Uptime: ", Style::default()),
                Span::styled(uptime_str, Style::default().fg(Color::Magenta)),
            ]),
        ];

        let metrics_paragraph = Paragraph::new(metrics_text)
            .block(Block::default().borders(Borders::ALL).title("System Metrics"))
            .wrap(Wrap { trim: true });

        frame.render_widget(metrics_paragraph, chunks[0]);

        // Right side - Resource usage gauges
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .split(chunks[1]);

        // CPU usage gauge
        let cpu_gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("CPU Usage"))
            .gauge_style(Style::default().fg(
                if self.metrics.cpu_usage > 80.0 { Color::Red }
                else if self.metrics.cpu_usage > 60.0 { Color::Yellow }
                else { Color::Green }
            ))
            .percent(self.metrics.cpu_usage as u16)
            .label(format!("{:.1}%", self.metrics.cpu_usage));

        frame.render_widget(cpu_gauge, right_chunks[0]);

        // Memory usage gauge
        let memory_gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Memory Usage"))
            .gauge_style(Style::default().fg(
                if memory_percent > 85.0 { Color::Red }
                else if memory_percent > 70.0 { Color::Yellow }
                else { Color::Green }
            ))
            .percent(memory_percent as u16)
            .label(format!("{:.1}% ({} MB)", memory_percent, self.metrics.memory_usage / 1024 / 1024));

        frame.render_widget(memory_gauge, right_chunks[1]);
    }

    fn render_agent_summary(&self, frame: &mut Frame, area: Rect) {
        let mut status_counts = HashMap::new();
        let mut total_messages = 0u64;
        let mut total_errors = 0u32;

        for agent in &self.agents {
            let status_key = match agent.status {
                AgentStatus::Running => "Running",
                AgentStatus::Idle => "Idle",
                AgentStatus::Starting => "Starting",
                AgentStatus::Stopping => "Stopping",
                AgentStatus::Stopped => "Stopped",
                AgentStatus::Error(_) => "Error",
            };
            *status_counts.entry(status_key).or_insert(0) += 1;
            total_messages += agent.metrics.messages_scraped;
            total_errors += agent.metrics.error_count;
        }

        let summary_text = vec![
            Line::from(vec![
                Span::styled("Agent Status Summary:", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  Running: ", Style::default()),
                Span::styled(
                    format!("{}", status_counts.get("Running").unwrap_or(&0)),
                    Style::default().fg(Color::Green),
                ),
                Span::styled("  Idle: ", Style::default()),
                Span::styled(
                    format!("{}", status_counts.get("Idle").unwrap_or(&0)),
                    Style::default().fg(Color::Blue),
                ),
                Span::styled("  Error: ", Style::default()),
                Span::styled(
                    format!("{}", status_counts.get("Error").unwrap_or(&0)),
                    Style::default().fg(Color::Red),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Total Messages Scraped: ", Style::default()),
                Span::styled(
                    format!("{}", total_messages),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("Total Errors: ", Style::default()),
                Span::styled(
                    format!("{}", total_errors),
                    Style::default().fg(if total_errors > 0 { Color::Red } else { Color::Green }),
                ),
            ]),
        ];
        let summary_paragraph = Paragraph::new(summary_text)
            .block(Block::default().borders(Borders::ALL).title("Agent Summary"))
            .wrap(Wrap { trim: true });

        frame.render_widget(summary_paragraph, area);
    }

    fn render_recent_activity(&self, frame: &mut Frame, area: Rect) {
        let recent_logs: Vec<ListItem> = self.logs.iter().rev()
            .take(area.height.saturating_sub(2) as usize)
            .map(|log| {
                let time_str = log.timestamp.format("%H:%M:%S").to_string();
                let agent_str = if let Some(agent_id) = log.agent_id {
                    format!("[{}]", &agent_id.to_string()[..8])
                } else {
                    "[SYSTEM]".to_string()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(time_str, Style::default().fg(Color::Gray)),
                    Span::raw(" "),
                    Span::styled(log.level.symbol(), Style::default().fg(log.level.color())),
                    Span::raw(" "),
                    Span::styled(agent_str, Style::default().fg(Color::Blue)),
                    Span::raw(" "),
                    Span::raw(&log.message),
                ]))
            })
            .collect();

        let activity_list = List::new(recent_logs)
            .block(Block::default().borders(Borders::ALL).title("Recent Activity"));

        frame.render_widget(activity_list, area);
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        // Agent table
        let header_cells = ["ID", "Streamer", "Status", "Messages", "Errors", "Uptime", "Last Message"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().add_modifier(Modifier::BOLD)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows: Vec<Row> = self.agents.iter().map(|agent| {
            let id_string = agent.id.to_string();
            let id_short = id_string[..8].to_string();
            let status_style = match agent.status {
                AgentStatus::Running => Style::default().fg(Color::Green),
                AgentStatus::Error(_) => Style::default().fg(Color::Red),
                AgentStatus::Starting => Style::default().fg(Color::Yellow),
                AgentStatus::Stopping => Style::default().fg(Color::Yellow),
                AgentStatus::Stopped => Style::default().fg(Color::Gray),
                AgentStatus::Idle => Style::default().fg(Color::Blue),
            };
            
            let status_text = match &agent.status {
                AgentStatus::Error(msg) => format!("Error: {}", msg),
                other => format!("{:?}", other),
            };
            
            let uptime_str = format_duration(agent.metrics.uptime);
            let last_message_str = if let Some(last_time) = agent.metrics.last_message_time {
                let elapsed = chrono::Utc::now().signed_duration_since(last_time);
                if elapsed.num_seconds() < 60 {
                    format!("{}s ago", elapsed.num_seconds())
                } else {
                    format!("{}m ago", elapsed.num_minutes())
                }
            } else {
                "N/A".to_string()
            };

            let cells = vec![
                Cell::from(id_short),
                Cell::from(agent.streamer.clone()),
                Cell::from(status_text).style(status_style),
                Cell::from(agent.metrics.messages_scraped.to_string()),
                Cell::from(agent.metrics.error_count.to_string()),
                Cell::from(uptime_str),
                Cell::from(last_message_str),
            ];
            Row::new(cells)
        }).collect();

        let table = Table::new(rows)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .widths(&[
                Constraint::Length(5),
                Constraint::Length(20),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(12),
                Constraint::Min(20),
            ]);

        frame.render_stateful_widget(table, area, &mut self.agent_table_state);
    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let log_items: Vec<ListItem> = self
            .logs
            .iter()
            .rev()
            .map(|log| {
                let time_str = log.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                let style = match log.level {
                    LogLevel::Info => Style::default().fg(self.theme.success_color),
                    LogLevel::Warning => Style::default().fg(self.theme.warning_color),
                    LogLevel::Error => Style::default().fg(self.theme.error_color),
                    LogLevel::Debug => Style::default().fg(self.theme.secondary_color),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", log.level.symbol()), style),
                    Span::raw(format!("[{}] ", time_str)),
                    Span::raw(log.message.clone()),
                ]))
            })
            .collect();

        let logs_list = List::new(log_items)
            .block(Block::default().borders(Borders::ALL).title("System Logs"))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");

        frame.render_stateful_widget(logs_list, area, &mut self.log_list_state);
    }

    fn render_performance(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);

        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[0]);

        self.render_cpu_graph(frame, top_chunks[0]);
        self.render_memory_graph(frame, top_chunks[1]);
        self.render_message_rate_graph(frame, chunks[1]);
    }

    fn render_cpu_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self
            .performance_history
            .iter()
            .enumerate()
            .map(|(i, data)| (i as f64, data.cpu_usage as f64))
            .collect();

        let datasets = vec![Dataset::default()
            .name("CPU Usage (%)")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .graph_type(GraphType::Line)
            .data(&data)];

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title("CPU Usage")
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, self.performance_history.len() as f64]),
            )
            .y_axis(
                Axis::default()
                    .title("Usage (%)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 100.0])
                    .labels(vec![
                        Span::raw("0"),
                        Span::raw("50"),
                        Span::raw("100"),
                    ]),
            );

        frame.render_widget(chart, area);
    }

    fn render_memory_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self
            .performance_history
            .iter()
            .enumerate()
            .map(|(i, data)| (i as f64, data.memory_usage as f64 / 1_000_000.0))
            .collect();

        let total_memory_mb = self.metrics.memory_total as f64 / 1_000_000.0;

        let datasets = vec![Dataset::default()
            .name("Memory Usage (MB)")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .graph_type(GraphType::Line)
            .data(&data)];

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title("Memory Usage")
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, self.performance_history.len() as f64]),
            )
            .y_axis(
                Axis::default()
                    .title("Usage (MB)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, total_memory_mb])
                    .labels(vec![
                        Span::raw("0"),
                        Span::raw(format!("{:.0}", total_memory_mb / 2.0)),
                        Span::raw(format!("{:.0}", total_memory_mb)),
                    ]),
            );

        frame.render_widget(chart, area);
    }

    fn render_message_rate_graph(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self
            .performance_history
            .iter()
            .enumerate()
            .map(|(i, data)| (i as f64, data.messages_per_second))
            .collect();

        let datasets = vec![Dataset::default()
            .name("Messages/sec")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::default().fg(self.theme.accent_color))
            .graph_type(GraphType::Line)
            .data(&data)];

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title("Message Rate")
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, self.performance_history.len() as f64]),
            )
            .y_axis(
                Axis::default()
                    .title("Rate")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, self.metrics.messages_per_second.max(10.0)]), // Dynamic Y-axis
            );

        frame.render_widget(chart, area);
    }

    fn render_performance_metrics(&self, frame: &mut Frame, area: Rect) {
        let metrics_text = vec![
            Line::from(vec![
                Span::styled("CPU:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" {:.2}%", self.metrics.cpu_usage)),
            ]),
            Line::from(vec![
                Span::styled("Memory:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(
                    " {:.2} MB / {:.2} MB",
                    self.metrics.memory_usage as f64 / 1_000_000.0,
                    self.metrics.memory_total as f64 / 1_000_000.0
                )),
            ]),
            Line::from(vec![
                Span::styled(
                    "Msg Rate:",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {:.2}/s", self.metrics.messages_per_second)),
            ]),
            Line::from(vec![
                Span::styled("Uptime:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" {}", format_duration(self.metrics.uptime))),
            ]),
            Line::from(vec![
                Span::styled("Active Agents:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" {}", self.metrics.active_agents)),
            ]),
            Line::from(vec![
                Span::styled("Total Msgs:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" {}", self.metrics.total_messages)),
            ]),
        ];

        let metrics_paragraph = Paragraph::new(metrics_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Performance Metrics"),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(metrics_paragraph, area);
    }

    fn render_alerts(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["ID", "Time", "Level", "Title", "Message"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(self.theme.accent_color)));
        let header = Row::new(header_cells)
            .style(Style::default().add_modifier(Modifier::BOLD))
            .height(1)
            .bottom_margin(1);

        let rows: Vec<Row> = self.alerts.iter().rev().map(|alert| {
            let time_str = alert.timestamp.format("%H:%M:%S").to_string();
            let level_style = Style::default().fg(alert.level.color());
            let agent_id_str = alert.agent_id.map_or_else(|| "N/A".to_string(), |id| id.to_string());

            let cells = vec![
                Cell::from(time_str),
                Cell::from(alert.level.symbol()).style(level_style),
                Cell::from(agent_id_str),
                Cell::from(alert.title.clone()),
                Cell::from(alert.message.clone()),
            ];
            Row::new(cells)
        }).collect();

        let table = Table::new(rows)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Alerts"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .widths(&[
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(20),
                Constraint::Min(30),
            ]);

        frame.render_stateful_widget(table, area, &mut self.alert_table_state);
    }

    fn render_help_popup(&self, frame: &mut Frame) {
        let area = centered_rect(60, 70, frame.size());
        
        frame.render_widget(Clear, area);

        let help_text = vec![
            Line::from(Span::styled("Help - Keybindings", Style::default().add_modifier(Modifier::BOLD).fg(self.theme.accent_color))),
            Line::from(""),
            Line::from(vec![Span::styled("q", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Quit the application")]),
            Line::from(vec![Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Cycle through tabs")]),
            Line::from(vec![Span::styled("h or ?", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Show/Hide this help popup")]),
            Line::from(""),
            Line::from(Span::styled("Agents Tab:", Style::default().add_modifier(Modifier::BOLD).fg(self.theme.accent_color))),
            Line::from(vec![Span::styled("↑/↓", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Navigate agent list")]),
            Line::from(vec![Span::styled("s", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Stop selected agent")]),
            Line::from(vec![Span::styled("r", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Restart selected agent")]),
            Line::from(vec![Span::styled("n", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Start a new agent (not implemented)")]),
            Line::from(""),
            Line::from(Span::styled("Logs/Alerts Tab:", Style::default().add_modifier(Modifier::BOLD).fg(self.theme.accent_color))),
            Line::from(vec![Span::styled("↑/↓", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Scroll through items")]),
            Line::from(vec![Span::styled("a", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": Acknowledge selected alert")]),
            Line::from(""),
            Line::from(Span::styled("Press any key to close...", Style::default().fg(self.theme.secondary_color))),
        ];

        let help_paragraph = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });

        frame.render_widget(help_paragraph, area);
    }
    
    fn handle_agents_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Up => {
                let selected = self.agent_table_state.selected().unwrap_or(0);
                let new_selected = if selected > 0 { selected - 1 } else { self.agents.len() - 1 };
                self.agent_table_state.select(Some(new_selected));
            }
            KeyCode::Down => {
                let selected = self.agent_table_state.selected().unwrap_or(0);
                let new_selected = if selected < self.agents.len() - 1 { selected + 1 } else { 0 };
                self.agent_table_state.select(Some(new_selected));
            }
            KeyCode::Char('s') => {
                if let Some(selected) = self.agent_table_state.selected() {
                    if let Some(agent) = self.agents.get(selected) {
                        return Ok(Action::StopAgent(agent.id.clone()));
                    }
                }
            }
            KeyCode::Char('r') => {
                if let Some(selected) = self.agent_table_state.selected() {
                    if let Some(agent) = self.agents.get(selected) {
                        return Ok(Action::RestartAgent(agent.id.clone()));
                    }
                }
            }
            _ => {}
        }
        Ok(Action::Continue)
    }

    fn handle_logs_input(&mut self, key_event: KeyEvent) -> Result<Action> {
        match key_event.code {
            KeyCode::Up => {
                let selected = self.log_list_state.selected().unwrap_or(0);
                if selected > 0 {
                    self.log_list_state.select(Some(selected - 1));
                }
            }
            KeyCode::Down => {
                let selected = self.log_list_state.selected().unwrap_or(0);
                let total_logs = self.logs.len();
                if total_logs > 0 && selected < total_logs - 1 {
                    self.log_list_state.select(Some(selected + 1));
                }
            }
            _ => {}
        }
        Ok(Action::Continue)
    }

    fn render(&mut self, frame: &mut Frame) -> Result<()> {
        // Update message rate calculation
        self.update_message_rate();
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(frame.size());

        let tab_titles = [
            Tab::Overview,
            Tab::Agents,
            Tab::Logs,
            Tab::Performance,
            Tab::Alerts,
        ]
        .iter()
        .map(|t| t.title())
        .collect();

        let tabs = Tabs::new(tab_titles)
            .block(Block::default().borders(Borders::ALL).title("Dashboard"))
            .select(self.current_tab as usize)
            .style(Style::default().fg(self.theme.text_color))
            .highlight_style(Style::default().fg(self.theme.accent_color).add_modifier(Modifier::BOLD));

        frame.render_widget(tabs, chunks[0]);

        match self.current_tab {
            Tab::Overview => self.render_overview(frame, chunks[1]),
            Tab::Agents => self.render_agents(frame, chunks[1]),
            Tab::Logs => self.render_logs(frame, chunks[1]),
            Tab::Performance => self.render_performance(frame, chunks[1]),
            Tab::Alerts => self.render_alerts(frame, chunks[1]),
        }

        if self.show_help {
            self.render_help_popup(frame);
        }

        Ok(())
    }

    fn handle_input(&mut self, event: Event) -> Result<Action> {
        if let Event::Key(key) = event {
            if self.show_help {
                self.show_help = false;
                return Ok(Action::Continue);
            }

            match key.code {
                KeyCode::Char('q') => return Ok(Action::Quit),
                KeyCode::Char('h') | KeyCode::Char('?') => self.show_help = true,
                KeyCode::Tab => {
                    let current_index = self.current_tab as usize;
                    let next_index = (current_index + 1) % 5;
                    self.current_tab = match next_index {
                        0 => Tab::Overview,
                        1 => Tab::Agents,
                        2 => Tab::Logs,
                        3 => Tab::Performance,
                        4 => Tab::Alerts,
                        _ => unreachable!(),
                    };
                }
                _ => {
                    return match self.current_tab {
                        Tab::Agents => self.handle_agents_input(key),
                        Tab::Logs => self.handle_logs_input(key),
                        _ => Ok(Action::Continue),
                    };
                }
            }
        }
        Ok(Action::Continue)
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
