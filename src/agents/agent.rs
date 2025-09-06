use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{interval, sleep, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use rand::Rng;

use crate::browser::{BrowserManager, BrowserInstanceId};
use crate::error::{Result, ScrapingError};
use crate::parser::chat_message::ChatMessage;
use crate::parser::html_parser::TwitchChatParser;

pub type AgentId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentStatus {
    Idle,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub messages_scraped: u64,
    pub uptime: Duration,
    pub error_count: u32,
    pub last_message_time: Option<DateTime<Utc>>,
    pub network_latency: Duration,
    pub memory_usage: u64,
    pub status: AgentStatus,
}

pub type MessageStream = tokio::sync::mpsc::Receiver<ChatMessage>;

#[async_trait]
pub trait Agent {
    async fn start(&mut self, streamer: &str) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn get_status(&self) -> AgentStatus;
    async fn get_metrics(&self) -> AgentMetrics;
    fn message_stream(&self) -> MessageStream;
}

pub struct ScrapingAgent {
    pub id: AgentId,
    pub streamer: Option<String>,
    pub status: Arc<RwLock<AgentStatus>>,
    pub metrics: Arc<RwLock<AgentMetrics>>,
    pub browser_manager: Option<Arc<BrowserManager>>,
    pub browser_instance_id: Option<BrowserInstanceId>,
    pub message_broadcaster: Option<broadcast::Sender<ChatMessage>>,
    pub start_time: Option<Instant>,
    pub parser: TwitchChatParser,
    pub shutdown_signal: Option<mpsc::Sender<()>>,
    pub monitoring_task: Option<tokio::task::JoinHandle<()>>,
    delay_range: (u64, u64),
}

impl ScrapingAgent {
    pub fn new(
        delay_range: (u64, u64),
        chat_message_broadcaster: broadcast::Sender<ChatMessage>,
    ) -> Result<Self> {
        let parser = TwitchChatParser::new()
            .map_err(|e| ScrapingError::AgentError(format!("Failed to create parser: {}", e)))?;

        Ok(Self {
            id: Uuid::new_v4(),
            streamer: None,
            status: Arc::new(RwLock::new(AgentStatus::Idle)),
            metrics: Arc::new(RwLock::new(AgentMetrics {
                messages_scraped: 0,
                uptime: Duration::from_secs(0),
                error_count: 0,
                last_message_time: None,
                network_latency: Duration::from_millis(0),
                memory_usage: 0,
                status: AgentStatus::Idle,
            })),
            browser_manager: None,
            browser_instance_id: None,
            message_broadcaster: Some(chat_message_broadcaster),
            start_time: None,
            parser,
            shutdown_signal: None,
            monitoring_task: None,
            delay_range,
        })
    }

    pub fn with_browser_manager(mut self, browser_manager: Arc<BrowserManager>) -> Self {
        self.browser_manager = Some(browser_manager);
        self
    }

    pub async fn initialize_browser(&mut self) -> Result<()> {
        if let Some(ref browser_manager) = self.browser_manager {
            let instance_id = browser_manager.create_browser_instance().await?;
            self.browser_instance_id = Some(instance_id);
            tracing::info!(
                "Initialized browser instance {} for agent {}",
                instance_id,
                self.id
            );
            Ok(())
        } else {
            Err(ScrapingError::AgentError("No browser manager available".to_string()).into())
        }
    }

    pub async fn cleanup_browser(&mut self) -> Result<()> {
        if let (Some(ref browser_manager), Some(instance_id)) =
            (&self.browser_manager, self.browser_instance_id)
        {
            browser_manager.remove_browser_instance(instance_id).await?;
            self.browser_instance_id = None;
            tracing::info!(
                "Cleaned up browser instance {} for agent {}",
                instance_id,
                self.id
            );
        }
        Ok(())
    }

    pub async fn update_uptime(&self) {
        if let Some(start_time) = self.start_time {
            let uptime = start_time.elapsed();
            let mut metrics = self.metrics.write().await;
            metrics.uptime = uptime;
        }
    }

    pub async fn increment_error_count(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.error_count += 1;
    }

    pub async fn update_message_metrics(&self, message_count: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.messages_scraped += message_count;
        metrics.last_message_time = Some(Utc::now());
    }

    pub async fn set_status(&self, status: AgentStatus) {
        let mut current_status = self.status.write().await;
        *current_status = status.clone();

        let mut metrics = self.metrics.write().await;
        metrics.status = status;
    }

    /// Start the real-time message extraction loop
    async fn start_message_monitoring(&mut self, streamer: String) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_signal = Some(shutdown_tx);

        let browser_manager = self
            .browser_manager
            .clone()
            .ok_or_else(|| ScrapingError::AgentError("No browser manager available".to_string()))?;
        let browser_instance_id = self
            .browser_instance_id
            .ok_or_else(|| ScrapingError::AgentError("No browser instance available".to_string()))?;
        let message_broadcaster = self
            .message_broadcaster
            .clone()
            .ok_or_else(|| ScrapingError::AgentError("No message broadcaster available".to_string()))?;

        let parser = TwitchChatParser::new()
            .map_err(|e| ScrapingError::AgentError(format!("Failed to create parser: {}", e)))?;
        let status = self.status.clone();
        let metrics = self.metrics.clone();
        let agent_id = self.id;
        let delay_range = self.delay_range;

        // Spawn the monitoring task
        let monitoring_task = tokio::spawn(async move {
            info!(
                "Starting message monitoring for agent {} on streamer {}",
                agent_id, streamer
            );

            let mut extraction_interval = interval(Duration::from_millis(1000));    // checking for new messages every 1000ms
            let mut last_html_hash = String::new();
            let mut consecutive_errors = 0;
            const MAX_CONSECUTIVE_ERRORS: u32 = 10;

            // initial random delay before starting
            let initial_delay = rand::thread_rng().gen_range(delay_range.0..=delay_range.1);
            info!(
                "Agent {} initial delay before extraction for {}ms",
                agent_id, initial_delay
            );
            sleep(Duration::from_millis(initial_delay)).await;

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Received shutdown signal for agent {}", agent_id);
                        break;
                    }
                    _ = extraction_interval.tick() => {
                        // Get browser instance and extract messages
                        if let Some(browser_instance) = browser_manager.get_browser_instance(browser_instance_id).await {
                            match Self::extract_and_process_messages(
                                &browser_instance,
                                &parser,
                                &streamer,
                                &mut last_html_hash,
                                &message_broadcaster,
                                &metrics
                            ).await {
                                Ok(message_count) => {
                                    consecutive_errors = 0;
                                    if message_count > 0 {
                                        debug!("Extracted {} messages for agent {}", message_count, agent_id);
                                    }
                                }
                                Err(e) => {
                                    consecutive_errors += 1;
                                    warn!("Error extracting messages for agent {}: {} (consecutive errors: {})",
                                          agent_id, e, consecutive_errors);

                                    if let Some(ScrapingError::BrowserError(_)) = e.downcast_ref::<ScrapingError>() {
                                        if let Some(browser_instance) = browser_manager.get_browser_instance(browser_instance_id).await {
                                            if let Some(_proxy) = browser_instance.proxy.clone() {
                                                // browser_manager.report_bad_proxy(proxy).await;
                                            }
                                        }
                                        error!("Browser error for agent {}, setting to error state", agent_id);
                                        let mut status_guard = status.write().await;
                                        *status_guard = AgentStatus::Error(format!("Browser error: {}", e));
                                        break; // Break from monitoring loop, orchestrator will restart
                                    }

                                    // Update error metrics
                                    let mut metrics_guard = metrics.write().await;
                                    metrics_guard.error_count += 1;
                                    drop(metrics_guard);

                                    // If too many consecutive errors, set agent to error state
                                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                        error!("Too many consecutive errors for agent {}, setting to error state", agent_id);
                                        let mut status_guard = status.write().await;
                                        *status_guard = AgentStatus::Error(format!("Too many consecutive errors: {}", e));
                                        break;
                                    }

                                    // exponential backoff on errors
                                    let backoff_duration = Duration::from_millis(1000 * (2_u64.pow(consecutive_errors.min(5))));
                                    sleep(backoff_duration).await;
                                }
                            }
                        } else {
                            error!("Browser instance not found for agent {}", agent_id);
                            let mut status_guard = status.write().await;
                            *status_guard = AgentStatus::Error("Browser instance not found".to_string());
                            break;
                        }
                    }
                }
            }

            info!("Message monitoring stopped for agent {}", agent_id);
        });

        self.monitoring_task = Some(monitoring_task);
        Ok(())
    }

    /// Extract and process messages from the current page
    async fn extract_and_process_messages(
        browser_instance: &crate::browser::BrowserInstance,
        parser: &TwitchChatParser,
        streamer: &str,
        last_html_hash: &mut String,
        message_broadcaster: &broadcast::Sender<ChatMessage>,
        metrics: &Arc<RwLock<AgentMetrics>>,
    ) -> Result<u64> {
        let start_time = Instant::now();

        // getting current html content
        let html = browser_instance.get_chat_html().await?;

        // simple hash to detect changes
        let current_hash = format!("{:x}", md5::compute(&html));
        if current_hash == *last_html_hash {
            return Ok(0); // No changes, skip processing
        }
        *last_html_hash = current_hash;

        // parsing messages from html
        let parsed_messages = parser.parse_chat_html(&html, streamer)?;
        let message_count = parsed_messages.len() as u64;

        // sending parsed messages directly
        for chat_message in parsed_messages {
            // Send message (non-blocking)
            if let Err(e) = message_broadcaster.send(chat_message) {
                match e {
                    broadcast::error::SendError(_) => {
                        warn!("No receivers for message broadcast, continuing");
                        // This is not an error condition - just means no one is listening
                    }
                }
            }
        }

        // updating metrics
        if message_count > 0 {
            let mut metrics_guard = metrics.write().await;
            metrics_guard.messages_scraped += message_count;
            metrics_guard.last_message_time = Some(Utc::now());
            metrics_guard.network_latency = start_time.elapsed();
        }

        Ok(message_count)
    }

    /// Stop the message monitoring task
    async fn stop_message_monitoring(&mut self) -> Result<()> {
        // Send shutdown signal
        if let Some(shutdown_tx) = self.shutdown_signal.take() {
            let _ = shutdown_tx.send(()).await;
        }

        // Wait for monitoring task to complete
        if let Some(task) = self.monitoring_task.take() {
            if let Err(e) = task.await {
                warn!("Error waiting for monitoring task to complete: {}", e);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Agent for ScrapingAgent {
    async fn start(&mut self, streamer: &str) -> Result<()> {
        info!("Starting agent {} for streamer {}", self.id, streamer);

        self.set_status(AgentStatus::Starting).await;
        self.streamer = Some(streamer.to_string());
        self.start_time = Some(Instant::now());

        // initialize browser if not done yet
        if self.browser_instance_id.is_none() {
            info!("Initializing browser for agent {}", self.id);
            match self.initialize_browser().await {
                Ok(_) => {
                    info!("Browser initialized successfully for agent {}", self.id);
                    // Add a small delay to let browser fully initialize
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
                Err(e) => {
                    error!("Failed to initialize browser for agent {}: {}", self.id, e);
                    self.set_status(AgentStatus::Error(format!("Browser init failed: {}", e))).await;
                    return Err(e);
                }
            }
        }

        // navigating to twitch stream
        if let (Some(ref browser_manager), Some(instance_id)) =
            (&self.browser_manager, self.browser_instance_id)
        {
            if let Some(browser_instance) = browser_manager.get_browser_instance(instance_id).await
            {
                match browser_instance.navigate_to_twitch_stream(streamer).await {
                    Ok(_) => {
                        // adding random delay after navigation
                        let delay =
                            rand::thread_rng().gen_range(self.delay_range.0..=self.delay_range.1);
                        info!("Agent {} delaying for {}ms after navigation", self.id, delay);
                        sleep(Duration::from_millis(delay)).await;

                        // starting the message monitoring loop
                        self.start_message_monitoring(streamer.to_string()).await?;

                        self.set_status(AgentStatus::Running).await;
                        info!(
                            "Agent {} successfully started for streamer {}",
                            self.id, streamer
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to navigate to Twitch stream for agent {}: {}",
                            self.id, e
                        );
                        if let Some(_proxy) = browser_instance.proxy.clone() {
                            // browser_manager.report_bad_proxy(proxy).await;
                        }
                        self.set_status(AgentStatus::Error(format!("Navigation failed: {}", e)))
                            .await;
                        return Err(e);
                    }
                }
            } else {
                self.set_status(AgentStatus::Error("Browser instance not found".to_string()))
                    .await;
                return Err(
                    ScrapingError::AgentError("Browser instance not found".to_string()).into(),
                );
            }
        } else {
            self.set_status(AgentStatus::Error(
                "No browser manager or instance available".to_string(),
            ))
            .await;
            return Err(ScrapingError::AgentError(
                "No browser manager or instance available".to_string(),
            )
            .into());
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Stopping agent {}", self.id);

        self.set_status(AgentStatus::Stopping).await;

        // stop message monitoring first
        self.stop_message_monitoring().await?;

        // cleaning up browser instance
        self.cleanup_browser().await?;

        self.set_status(AgentStatus::Stopped).await;
        self.start_time = None;

        info!("Agent {} stopped", self.id);
        Ok(())
    }

    async fn get_status(&self) -> AgentStatus {
        let status = self.status.read().await;
        status.clone()
    }

    async fn get_metrics(&self) -> AgentMetrics {
        // Update uptime before returning metrics
        self.update_uptime().await;

        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    fn message_stream(&self) -> MessageStream {
        // Create a new mpsc channel and spawn a task to forward broadcast messages
        let (tx, rx) = tokio::sync::mpsc::channel(1000);

        if let Some(broadcaster) = &self.message_broadcaster {
            let mut broadcast_rx = broadcaster.subscribe();
            let agent_id = self.id;

            tokio::spawn(async move {
                while let Ok(message) = broadcast_rx.recv().await {
                    if tx.send(message).await.is_err() {
                        debug!("Message stream receiver dropped for agent {}", agent_id);
                        break;
                    }
                }
            });
        }

        rx
    }
}
