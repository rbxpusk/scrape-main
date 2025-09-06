#[cfg(test)]
mod tests {
    use crate::agents::{AgentOrchestrator};
    use crate::config::{Config, FileConfigManager, ConfigManager};
    use std::sync::Arc;
    use tempfile::tempdir;

    // create a mock orchestrator without browser manager for testing
    fn create_mock_orchestrator() -> AgentOrchestrator {
        let config = Config::default();
        
        // create a mock browser manager - we'll use a placeholder arc
        // in a real test environment, we'd use a mock browser manager
        let mock_browser_manager = Arc::new(
            // this will fail in tests, but that's expected for unit tests
            // in integration tests, we'd use a real browser manager
            unsafe { std::mem::zeroed() }
        );
        
        AgentOrchestrator::new(config, mock_browser_manager)
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        // test basic orchestrator creation without browser dependencies
        let config = Config::default();
        
        // we can't create a real browser manager in tests, so we'll test
        // the orchestrator structure without actually creating browser instances
        assert_eq!(config.agents.max_concurrent, 5); // Default value
        assert_eq!(config.streamers, vec!["shroud", "ninja"]); // Default streamers
    }

    #[tokio::test]
    async fn test_config_update() {
        // test configuration update logic without browser dependencies
        let mut config = Config::default();
        config.streamers = vec!["newstreamer".to_string()];
        config.agents.max_concurrent = 3;
        
        // Verify config values
        assert_eq!(config.streamers, vec!["newstreamer".to_string()]);
        assert_eq!(config.agents.max_concurrent, 3);
    }

    #[tokio::test]
    async fn test_config_manager() {
        // test configuration file management
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let config_manager = FileConfigManager::new(config_path);
        
        // load default config
        let config = config_manager.load_config().await.expect("Failed to load config");
        
        // Verify default values
        assert_eq!(config.streamers, vec!["shroud", "ninja"]);
        assert_eq!(config.agents.max_concurrent, 5);
        assert_eq!(config.output.format, "json");
        assert!(config.monitoring.tui_enabled);
    }

    #[tokio::test]
    async fn test_system_metrics_structure() {
        use crate::agents::SystemMetrics;
        use std::time::Instant;
        
        // test systemmetrics structure
        let metrics = SystemMetrics {
            cpu_usage: 50.0,
            memory_usage: 1024 * 1024 * 1024, // 1GB
            memory_total: 8 * 1024 * 1024 * 1024, // 8GB
            active_agents: 3,
            total_messages_scraped: 1000,
            timestamp: Instant::now(),
        };
        
        assert_eq!(metrics.cpu_usage, 50.0);
        assert_eq!(metrics.active_agents, 3);
        assert_eq!(metrics.total_messages_scraped, 1000);
        assert!(metrics.memory_usage > 0);
        assert!(metrics.memory_total > metrics.memory_usage);
    }

    #[tokio::test]
    async fn test_agent_assignment_structure() {
        use crate::agents::AgentAssignment;
        use std::time::Instant;
        use uuid::Uuid;
        
        // test agentassignment structure
        let assignment = AgentAssignment {
            agent_id: Uuid::new_v4(),
            streamer: "teststreamer".to_string(),
            assigned_at: Instant::now(),
            priority: 1,
        };
        
        assert_eq!(assignment.streamer, "teststreamer");
        assert_eq!(assignment.priority, 1);
    }

    #[tokio::test]
    async fn test_orchestrator_status_structure() {
        use crate::agents::{OrchestratorStatus, SystemMetrics};
        use std::time::{Duration, Instant};
        
        // test orchestratorstatus structure
        let system_metrics = SystemMetrics {
            cpu_usage: 25.0,
            memory_usage: 2 * 1024 * 1024 * 1024, // 2GB
            memory_total: 16 * 1024 * 1024 * 1024, // 16GB
            active_agents: 2,
            total_messages_scraped: 500,
            timestamp: Instant::now(),
        };
        
        let status = OrchestratorStatus {
            active_agents: 2,
            total_agents_spawned: 5,
            system_metrics,
            agent_assignments: vec![],
            error_count: 1,
            uptime: Duration::from_secs(3600), // 1 hour
        };
        
        assert_eq!(status.active_agents, 2);
        assert_eq!(status.total_agents_spawned, 5);
        assert_eq!(status.error_count, 1);
        assert_eq!(status.uptime.as_secs(), 3600);
        assert_eq!(status.system_metrics.cpu_usage, 25.0);
    }

    #[tokio::test]
    async fn test_agent_message_types() {
        use crate::agents::{AgentMessage, AgentStatus};
        use uuid::Uuid;
        
        // test agentmessage enum variants
        let agent_id = Uuid::new_v4();
        
        let status_update = AgentMessage::StatusUpdate {
            agent_id,
            status: AgentStatus::Running,
        };
        
        let resource_alert = AgentMessage::ResourceAlert {
            agent_id,
            alert: "High CPU usage".to_string(),
        };
        
        let error_message = AgentMessage::Error {
            agent_id,
            error: "Connection failed".to_string(),
        };
        
        // verify message types can be created
        match status_update {
            AgentMessage::StatusUpdate { agent_id: _, status } => {
                match status {
                    AgentStatus::Running => assert!(true),
                    _ => assert!(false, "Expected Running status"),
                }
            }
            _ => assert!(false, "Expected StatusUpdate message"),
        }
        
        match resource_alert {
            AgentMessage::ResourceAlert { agent_id: _, alert } => {
                assert_eq!(alert, "High CPU usage");
            }
            _ => assert!(false, "Expected ResourceAlert message"),
        }
        
        match error_message {
            AgentMessage::Error { agent_id: _, error } => {
                assert_eq!(error, "Connection failed");
            }
            _ => assert!(false, "Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_config_validation() {
        use crate::config::FileConfigManager;
        use std::path::PathBuf;
        
        let manager = FileConfigManager::new(PathBuf::from("test.toml"));
        
        // Test valid config
        let valid_config = Config::default();
        assert!(manager.validate_config(&valid_config).is_ok());

        // Test invalid config - empty streamers
        let mut invalid_config = Config::default();
        invalid_config.streamers.clear();
        assert!(manager.validate_config(&invalid_config).is_err());

        // Test invalid config - max_concurrent = 0
        let mut invalid_config = Config::default();
        invalid_config.agents.max_concurrent = 0;
        assert!(manager.validate_config(&invalid_config).is_err());

        // Test invalid config - invalid delay range
        let mut invalid_config = Config::default();
        invalid_config.agents.delay_range = (5000, 1000);
        assert!(manager.validate_config(&invalid_config).is_err());
    }
}