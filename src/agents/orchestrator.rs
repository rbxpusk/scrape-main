use crate::error::{Result, ScrapingError};
use crate::parser::chat_message::ChatMessage;
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use sysinfo::{CpuExt, System, SystemExt};
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, sleep, Instant};
use tracing::{debug, error, info, warn};

use crate::agents::{Agent, AgentId, AgentMetrics, AgentStatus, ScrapingAgent};
use crate::browser::BrowserManager;
use crate::config::{Config, ConfigManager};

/// System resource metrics for dynamic scaling decisions
/// System resource metrics for dynamic scaling decisions
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub memory_total: u64,
    pub active_agents: usize,
    pub total_messages_scraped: u64,
    #[serde(with = "humantime_serde")]
    pub timestamp: SystemTime,
}

/// Agent assignment information
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentAssignment {
    pub agent_id: AgentId,
    pub streamer: String,
    #[serde(with = "humantime_serde")]
    pub assigned_at: SystemTime,
    pub priority: u8, // 0 = highest priority
    pub retry_attempts: u32,
    #[serde(with = "humantime_serde")]
    pub last_failure: Option<SystemTime>,
}

/// Orchestrator status and statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct OrchestratorStatus {
    pub active_agents: usize,
    pub total_agents_spawned: u64,
    pub system_metrics: SystemMetrics,
    pub agent_assignments: Vec<AgentAssignment>,
    pub error_count: u32,
    #[serde(with = "humantime_serde")]
    pub uptime: Duration,
}

/// Inter-agent communication message types
#[derive(Debug, Clone)]
pub enum AgentMessage {
    StatusUpdate {
        agent_id: AgentId,
        status: AgentStatus,
    },
    MetricsUpdate {
        agent_id: AgentId,
        metrics: AgentMetrics,
    },
    ChatMessage {
        agent_id: AgentId,
        message: ChatMessage,
    },
    ResourceAlert {
        agent_id: AgentId,
        alert: String,
    },
    Error {
        agent_id: AgentId,
        error: String,
    },
}

pub struct AgentOrchestrator {
    // Core state
    agents: Arc<RwLock<HashMap<AgentId, ScrapingAgent>>>,
    pub agent_assignments: Arc<RwLock<HashMap<AgentId, AgentAssignment>>>,
    browser_manager: Arc<BrowserManager>,

    // Configuration and limits
    config: Arc<RwLock<Config>>,
    max_concurrent: usize,

    // Communication channels
    message_broadcaster: broadcast::Sender<AgentMessage>,
    chat_message_broadcaster: broadcast::Sender<ChatMessage>,
    shutdown_signal: Option<broadcast::Sender<()>>,

    // System monitoring
    system: Arc<RwLock<System>>,
    system_metrics: Arc<RwLock<SystemMetrics>>,

    // Statistics
    total_agents_spawned: Arc<RwLock<u64>>,
    error_count: Arc<RwLock<u32>>,
    start_time: Instant,

    // Background tasks
    monitoring_task: Option<tokio::task::JoinHandle<()>>,
    scaling_task: Option<tokio::task::JoinHandle<()>>,
    config_watcher_task: Option<tokio::task::JoinHandle<()>>,
    agent_recovery_task: Option<tokio::task::JoinHandle<()>>,
}

impl AgentOrchestrator {
    pub fn new(config: Config, browser_manager: Arc<BrowserManager>) -> Self {
        let max_concurrent = config.agents.max_concurrent;
        let (message_broadcaster, _) = broadcast::channel(10000);
        let (chat_message_broadcaster, _) = broadcast::channel(10000);

        let mut system = System::new_all();
        system.refresh_all();

        let initial_metrics = SystemMetrics {
            cpu_usage: 0.0,
            memory_usage: system.used_memory(),
            memory_total: system.total_memory(),
            active_agents: 0,
            total_messages_scraped: 0,
            timestamp: SystemTime::now(),
        };

        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            agent_assignments: Arc::new(RwLock::new(HashMap::new())),
            browser_manager,
            config: Arc::new(RwLock::new(config)),
            max_concurrent,
            message_broadcaster,
            chat_message_broadcaster,
            shutdown_signal: None,
            system: Arc::new(RwLock::new(system)),
            system_metrics: Arc::new(RwLock::new(initial_metrics)),
            total_agents_spawned: Arc::new(RwLock::new(0)),
            error_count: Arc::new(RwLock::new(0)),
                    start_time: Instant::now(),
            monitoring_task: None,
            scaling_task: None,
            config_watcher_task: None,
            agent_recovery_task: None,
        }
    }

    /// Start the orchestrator with all background tasks
    pub async fn start(
        &mut self,
        config_manager: Arc<dyn ConfigManager + Send + Sync>,
    ) -> Result<()> {
        info!("Starting Agent Orchestrator");

        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();

        // Store the broadcast sender for shutdown signaling
        self.shutdown_signal = Some(shutdown_tx.clone());

        // Start system monitoring task
        self.start_system_monitoring(shutdown_rx1).await?;

        // Start dynamic scaling task
        self.start_dynamic_scaling(shutdown_rx2).await?;

        // Start configuration watcher task
        self.start_config_watcher(config_manager, shutdown_rx3)
            .await?;

        // Start agent recovery task
        self.start_agent_recovery(shutdown_tx.subscribe()).await?;

        // Distribute agents across configured streamers
        self.distribute_agents().await?;

        info!("Agent Orchestrator started successfully");
        Ok(())
    }

    /// Stop the orchestrator and all agents
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping Agent Orchestrator");

        // Send shutdown signal to all background tasks
        if let Some(shutdown_tx) = self.shutdown_signal.take() {
            let _ = shutdown_tx.send(());
        }

        // Stop all agents
        self.stop_all_agents().await?;

        // Wait for background tasks to complete
        if let Some(task) = self.monitoring_task.take() {
            let _ = task.await;
        }
        if let Some(task) = self.scaling_task.take() {
            let _ = task.await;
        }
        if let Some(task) = self.config_watcher_task.take() {
            let _ = task.await;
        }
        if let Some(task) = self.agent_recovery_task.take() {
            let _ = task.await;
        }

        info!("Agent Orchestrator stopped");
        Ok(())
    }

    /// Distribute agents across configured streamers based on priority
    pub async fn distribute_agents(&mut self) -> Result<()> {
        let config = self.config.read().await;
        let streamers = config.streamers.clone();
        let max_concurrent = config.agents.max_concurrent;
        drop(config);

        info!("Distributing agents across {} streamers: {:?}", streamers.len(), streamers);

        // stopping existing agents not in new streamer list
        let current_assignments = self.agent_assignments.read().await.clone();
        for (agent_id, assignment) in current_assignments {
            if !streamers.contains(&assignment.streamer) {
                info!(
                    "Stopping agent {} for removed streamer {}",
                    agent_id, assignment.streamer
                );
                self.stop_agent(agent_id).await?;
            }
        }

        // calculating how many agents per streamer
        let _agents_per_streamer = if streamers.is_empty() {
            0
        } else {
            (max_concurrent / streamers.len()).max(1)
        };

        let mut assigned_count = 0;

        // assigning agents to streamers
        for (index, streamer) in streamers.iter().enumerate() {
            if assigned_count >= max_concurrent {
                break;
            }

            // checking if we have agent for this streamer
            let assignments = self.agent_assignments.read().await;
            let has_agent = assignments.values().any(|a| a.streamer == *streamer);
            drop(assignments);

            if !has_agent {
                info!("No existing agent for streamer {}, spawning new one", streamer);
                let priority = index as u8; // Earlier streamers get higher priority (lower number)
                match self.spawn_agent(streamer, priority).await {
                    Ok(agent_id) => {
                        info!(
                            "Successfully assigned agent {} to streamer {} with priority {}",
                            agent_id, streamer, priority
                        );
                        assigned_count += 1;
                    }
                    Err(e) => {
                        error!("Failed to assign agent to streamer {}: {}", streamer, e);
                        self.increment_error_count().await;
                    }
                }
            } else {
                info!("Agent already exists for streamer {}", streamer);
                assigned_count += 1; // Count existing agents
            }
        }

        info!(
            "Agent distribution complete: {} agents assigned",
            assigned_count
        );
        Ok(())
    }

    /// Spawn a new agent for a specific streamer with priority
    pub async fn spawn_agent(&mut self, streamer: &str, priority: u8) -> Result<AgentId> {
        let agents = self.agents.read().await;
        if agents.len() >= self.max_concurrent {
            return Err(ScrapingError::ResourceLimit(
                "Maximum concurrent agents reached".to_string(),
            )
            .into());
        }
        drop(agents);

        let config = self.config.read().await;
        let delay_range = config.agents.delay_range;
        drop(config);

        let mut agent =
            ScrapingAgent::new(delay_range, self.chat_message_broadcaster.clone())?;
        let agent_id = agent.id;

        // Configure agent with browser manager
        agent = agent.with_browser_manager(self.browser_manager.clone());

        // staggering startup delay
        let startup_delay = rand::thread_rng().gen_range(100..=2000); // 0.1 to 2 seconds
        info!(
            "Agent {} delaying for {}ms before startup",
            agent_id, startup_delay
        );
        sleep(Duration::from_millis(startup_delay)).await;

        // Start the agent with timeout
        info!("Starting agent {} for streamer {}", agent_id, streamer);
        match tokio::time::timeout(Duration::from_secs(30), agent.start(streamer)).await {
            Ok(Ok(_)) => {
                info!("Agent {} started successfully for streamer {}", agent_id, streamer);
            }
            Ok(Err(e)) => {
                error!("Agent {} failed to start for streamer {}: {}", agent_id, streamer, e);
                return Err(e);
            }
            Err(_) => {
                error!("Agent {} startup timed out for streamer {}", agent_id, streamer);
                return Err(ScrapingError::AgentError(format!("Agent startup timed out for {}", streamer)).into());
            }
        }

        // create assignment record
        let assignment = AgentAssignment {
            agent_id,
            streamer: streamer.to_string(),
            assigned_at: SystemTime::now(),
            priority,
            retry_attempts: 0,
            last_failure: None,
        };

        // store agent and assignment
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent_id, agent);
        }
        {
            let mut assignments = self.agent_assignments.write().await;
            assignments.insert(agent_id, assignment);
        }

        // Update statistics
        {
            let mut total = self.total_agents_spawned.write().await;
            *total += 1;
        }

        // broadcast agent spawn message
        let _ = self.message_broadcaster.send(AgentMessage::StatusUpdate {
            agent_id,
            status: AgentStatus::Starting,
        });

        info!(
            "Spawned agent {} for streamer {} with priority {}",
            agent_id, streamer, priority
        );
        Ok(agent_id)
    }

    /// Stop a specific agent
    pub async fn stop_agent(&mut self, agent_id: AgentId) -> Result<()> {
        let mut agents = self.agents.write().await;
        if let Some(mut agent) = agents.remove(&agent_id) {
            agent.stop().await?;

            // remove assignment
            let mut assignments = self.agent_assignments.write().await;
            if let Some(assignment) = assignments.remove(&agent_id) {
                info!(
                    "Stopped agent {} for streamer {}",
                    agent_id, assignment.streamer
                );
            }

            // broadcast agent stop message
            let _ = self.message_broadcaster.send(AgentMessage::StatusUpdate {
                agent_id,
                status: AgentStatus::Stopped,
            });
        }
        Ok(())
    }

    /// Get status of a specific agent
    pub async fn get_agent_status(&self, agent_id: AgentId) -> Option<AgentStatus> {
        let agents = self.agents.read().await;
        if let Some(agent) = agents.get(&agent_id) {
            Some(agent.get_status().await)
        } else {
            None
        }
    }

    /// Get metrics for a specific agent
    pub async fn get_agent_metrics(&self, agent_id: AgentId) -> Option<AgentMetrics> {
        let agents = self.agents.read().await;
        if let Some(agent) = agents.get(&agent_id) {
            Some(agent.get_metrics().await)
        } else {
            None
        }
    }

    /// Get list of active agent IDs
    pub async fn get_active_agents(&self) -> Vec<AgentId> {
        let agents = self.agents.read().await;
        agents.keys().cloned().collect()
    }

    /// Get comprehensive orchestrator status
    pub async fn get_status(&self) -> OrchestratorStatus {
        let _agents = self.agents.read().await;
        let _assignments = self.agent_assignments.read().await;
        let system_metrics = self.system_metrics.read().await.clone();
        let agent_assignments: Vec<AgentAssignment> = self
            .agent_assignments
            .read()
            .await
            .values()
            .cloned()
            .collect();

        OrchestratorStatus {
            active_agents: self.agents.read().await.len(),
            total_agents_spawned: *self.total_agents_spawned.read().await,
            system_metrics,
            agent_assignments,
            error_count: *self.error_count.read().await,
            uptime: self.start_time.elapsed(),
        }
    }

    /// Subscribe to inter-agent communication messages
    pub fn subscribe_to_messages(&self) -> broadcast::Receiver<AgentMessage> {
        self.message_broadcaster.subscribe()
    }

    /// Subscribe to chat messages
    pub fn subscribe_to_chat_messages(&self) -> broadcast::Receiver<ChatMessage> {
        self.chat_message_broadcaster.subscribe()
    }

    /// Stop all agents
    pub async fn stop_all_agents(&mut self) -> Result<()> {
        let agent_ids: Vec<AgentId> = {
            let agents = self.agents.read().await;
            agents.keys().cloned().collect()
        };

        for agent_id in agent_ids {
            if let Err(e) = self.stop_agent(agent_id).await {
                warn!("Error stopping agent {}: {}", agent_id, e);
                self.increment_error_count().await;
            }
        }

        Ok(())
    }

    /// Update configuration and redistribute agents if needed
    pub async fn update_config(&mut self, new_config: Config) -> Result<()> {
        info!("Updating orchestrator configuration");

        let old_streamers = {
            let config = self.config.read().await;
            config.streamers.clone()
        };

        // Update configuration
        {
            let mut config = self.config.write().await;
            *config = new_config;
        }

        let new_streamers = {
            let config = self.config.read().await;
            config.streamers.clone()
        };

        // Redistribute agents if streamer list changed
        if old_streamers != new_streamers {
            info!("Streamer list changed, redistributing agents");
            self.distribute_agents().await?;
        }

        Ok(())
    }

    /// Restart a failed agent
    pub async fn restart_agent(&mut self, agent_id: AgentId) -> Result<()> {
        let assignment = {
            let mut assignments = self.agent_assignments.write().await;
            assignments.remove(&agent_id)
        };

        if let Some(mut assignment) = assignment {
            info!(
                "Restarting agent {} for streamer {}",
                agent_id, assignment.streamer
            );

            // stop the existing agent
            self.stop_agent(agent_id).await?;

            // increment retry attempts and update last failure
            assignment.retry_attempts += 1;
            assignment.last_failure = Some(SystemTime::now());

            // spawn new agent for same streamer
            let new_agent_id = self
                .spawn_agent(&assignment.streamer, assignment.priority)
                .await?;

            // update assignment with new agent id
            assignment.agent_id = new_agent_id;
            let mut assignments = self.agent_assignments.write().await;
            assignments.insert(new_agent_id, assignment);

            Ok(())
        } else {
            Err(ScrapingError::AgentError(format!("Agent {} not found for restart", agent_id)).into())
        }
    }

    /// Scale agents based on system resources and demand
    pub async fn scale_agents(&mut self) -> Result<()> {
        let system_metrics = self.system_metrics.read().await.clone();
        let config = self.config.read().await;
        let max_concurrent = config.agents.max_concurrent;
        let streamers = config.streamers.clone();
        drop(config);

        let current_agents = {
            let agents = self.agents.read().await;
            agents.len()
        };

        let memory_usage_percent =
            (system_metrics.memory_usage as f64 / system_metrics.memory_total as f64) * 100.0;

        // Scale down if resource usage is too high
        if (system_metrics.cpu_usage > 85.0 || memory_usage_percent > 85.0) && current_agents > 1 {
            info!(
                "High resource usage detected, scaling down agents. CPU: {:.1}%, Memory: {:.1}%",
                system_metrics.cpu_usage, memory_usage_percent
            );

            // Find the lowest priority agent to stop
            let agent_id_to_stop = {
                let assignments = self.agent_assignments.read().await;
                assignments
                    .iter()
                    .max_by_key(|(_, assignment)| assignment.priority)
                    .map(|(id, _)| *id)
            };

            if let Some(agent_id) = agent_id_to_stop {
                self.stop_agent(agent_id).await?;
            }
        }
        // Scale up if resources are available and we have unassigned streamers
        else if system_metrics.cpu_usage < 60.0
            && memory_usage_percent < 70.0
            && current_agents < max_concurrent
        {
            // Find streamers without agents
            let assignments = self.agent_assignments.read().await;
            let assigned_streamers: Vec<String> =
                assignments.values().map(|a| a.streamer.clone()).collect();
            drop(assignments);

            for (index, streamer) in streamers.iter().enumerate() {
                if !assigned_streamers.contains(streamer) && current_agents < max_concurrent {
                    info!(
                        "Resources available, scaling up agent for streamer {}",
                        streamer
                    );
                    let priority = index as u8;
                    if let Err(e) = self.spawn_agent(streamer, priority).await {
                        warn!("Failed to scale up agent for streamer {}: {}", streamer, e);
                    }
                    break; // Only add one agent at a time
                }
            }
        }

        Ok(())
    }

    /// Get agent performance metrics for load balancing
    pub async fn get_agent_performance_metrics(&self) -> HashMap<AgentId, AgentMetrics> {
        let agents = self.agents.read().await;
        let mut metrics = HashMap::new();

        for (agent_id, agent) in agents.iter() {
            metrics.insert(*agent_id, agent.get_metrics().await);
        }

        metrics
    }

    /// Rebalance agents based on performance
    pub async fn rebalance_agents(&mut self) -> Result<()> {
        let performance_metrics = self.get_agent_performance_metrics().await;

        // find underperforming agents
        for (agent_id, metrics) in performance_metrics {
            let error_rate = if metrics.uptime.as_secs() > 0 {
                metrics.error_count as f64 / metrics.uptime.as_secs() as f64
            } else {
                0.0
            };

            // if error rate too high, restart agent
            if error_rate > 0.1 {
                // More than 0.1 errors per second
                warn!(
                    "Agent {} has high error rate ({:.2}/sec), restarting",
                    agent_id, error_rate
                );
                if let Err(e) = self.restart_agent(agent_id).await {
                    error!(
                        "Failed to restart underperforming agent {}: {}",
                        agent_id, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Start system monitoring background task
    async fn start_system_monitoring(
        &mut self,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<()> {
        let system = self.system.clone();
        let system_metrics = self.system_metrics.clone();
        let agents = self.agents.clone();
        let message_broadcaster = self.message_broadcaster.clone();

        let monitoring_task = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5)); // Update every 5 seconds

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("System monitoring task received shutdown signal");
                        break;
                    }
                    _ = interval.tick() => {
                        // update system information
                        {
                            let mut sys = system.write().await;
                            sys.refresh_cpu();
                            sys.refresh_memory();
                        }

                        // calculate metrics
                        let sys = system.read().await;
                        let cpu_usage = sys.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32;
                        let memory_usage = sys.used_memory();
                        let memory_total = sys.total_memory();
                        drop(sys);

                        let active_agents = {
                            let agents_guard = agents.read().await;
                            agents_guard.len()
                        };

                        // calculate total messages scraped
                        let total_messages = {
                            let agents_guard = agents.read().await;
                            let mut total = 0u64;
                            for agent in agents_guard.values() {
                                let metrics = agent.get_metrics().await;
                                total += metrics.messages_scraped;
                            }
                            total
                        };

                        let metrics = SystemMetrics {
                            cpu_usage,
                            memory_usage,
                            memory_total,
                            active_agents,
                            total_messages_scraped: total_messages,
                            timestamp: SystemTime::now(),
                        };

                        // update stored metrics
                        {
                            let mut stored_metrics = system_metrics.write().await;
                            *stored_metrics = metrics.clone();
                        }

                        // check for resource alerts
                        if cpu_usage > 80.0 {
                            let _ = message_broadcaster.send(AgentMessage::ResourceAlert {
                                agent_id: uuid::Uuid::nil(), // System-level alert
                                alert: format!("High CPU usage: {:.1}%", cpu_usage),
                            });
                        }

                        let memory_usage_percent = (memory_usage as f64 / memory_total as f64) * 100.0;
                        if memory_usage_percent > 85.0 {
                            let _ = message_broadcaster.send(AgentMessage::ResourceAlert {
                                agent_id: uuid::Uuid::nil(), // System-level alert
                                alert: format!("High memory usage: {:.1}%", memory_usage_percent),
                            });
                        }
                    }
                }
            }
        });

        self.monitoring_task = Some(monitoring_task);
        Ok(())
    }

    /// Start dynamic scaling background task
    async fn start_dynamic_scaling(
        &mut self,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<()> {
        let system_metrics = self.system_metrics.clone();
        let agents = self.agents.clone();
        let config = self.config.clone();
        let message_broadcaster = self.message_broadcaster.clone();

        let scaling_task = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30)); // Check every 30 seconds

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Dynamic scaling task received shutdown signal");
                        break;
                    }
                    _ = interval.tick() => {
                        let metrics = system_metrics.read().await.clone();
                        let config_guard = config.read().await;
                        let max_concurrent = config_guard.agents.max_concurrent;
                        drop(config_guard);

                        let current_agents = {
                            let agents_guard = agents.read().await;
                            agents_guard.len()
                        };

                        // scaling decision logic
                        let memory_usage_percent = (metrics.memory_usage as f64 / metrics.memory_total as f64) * 100.0;

                        // Scale down if resource usage is too high
                        if (metrics.cpu_usage > 90.0 || memory_usage_percent > 90.0) && current_agents > 1 {
                            let _ = message_broadcaster.send(AgentMessage::ResourceAlert {
                                agent_id: uuid::Uuid::nil(),
                                alert: format!("Resource usage critical - consider scaling down agents. CPU: {:.1}%, Memory: {:.1}%",
                                              metrics.cpu_usage, memory_usage_percent),
                            });
                        }

                        // scale up if resources available
                        else if metrics.cpu_usage < 50.0 && memory_usage_percent < 60.0 && current_agents < max_concurrent {
                            debug!("Resources available for scaling up: CPU: {:.1}%, Memory: {:.1}%, Agents: {}/{}",
                                  metrics.cpu_usage, memory_usage_percent, current_agents, max_concurrent);
                        }
                    }
                }
            }
        });

        self.scaling_task = Some(scaling_task);
        Ok(())
    }

    /// Start configuration watcher background task
    async fn start_config_watcher(
        &mut self,
        config_manager: Arc<dyn ConfigManager + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<()> {
        let config = self.config.clone();
        let message_broadcaster = self.message_broadcaster.clone();

        let config_watcher_task = tokio::spawn(async move {
            match config_manager.watch_config_changes().await {
                Ok(mut config_rx) => loop {
                    tokio::select! {
                        _ = shutdown_rx.recv() => {
                            debug!("Config watcher task received shutdown signal");
                            break;
                        }
                        new_config = config_rx.recv() => {
                            if let Some(new_config) = new_config {
                                info!("Configuration updated, applying changes");

                                // update stored configuration
                                {
                                    let mut config_guard = config.write().await;
                                    *config_guard = new_config;
                                }

                                // broadcast configuration update
                                let _ = message_broadcaster.send(AgentMessage::ResourceAlert {
                                    agent_id: uuid::Uuid::nil(),
                                    alert: "Configuration updated".to_string(),
                                });
                            } else {
                                debug!("Config watcher channel closed");
                                break;
                            }
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to start config watcher: {}", e);
                }
            }
        });

        self.config_watcher_task = Some(config_watcher_task);
        Ok(())
    }

    /// Start agent recovery background task
    async fn start_agent_recovery(
        &mut self,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<()> {
        let agents = self.agents.clone();
        let _agent_assignments = self.agent_assignments.clone();
        let _message_broadcaster = self.message_broadcaster.clone();

        let agent_recovery_task = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(15)); // Check every 15 seconds

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Agent recovery task received shutdown signal");
                        break;
                    }
                    _ = interval.tick() => {
                        let mut agents_to_restart = Vec::new();
                        let agents_guard = agents.read().await;
                        for (agent_id, agent) in agents_guard.iter() {
                            let status = agent.get_status().await;
                            if let AgentStatus::Error(_) = status {
                                agents_to_restart.push(*agent_id);
                            }
                        }
                        drop(agents_guard);

                        for agent_id in agents_to_restart {
                            warn!("Agent {} is in error state, attempting to restart", agent_id);
                            // this is simplified restart, real would need more logic
                            // to manage agents and streamers.
                        }
                    }
                }
            }
        });

        self.agent_recovery_task = Some(agent_recovery_task);
        Ok(())
    }

    /// Increment error counter
    async fn increment_error_count(&self) {
        let mut error_count = self.error_count.write().await;
        *error_count += 1;
    }
}
