#[cfg(test)]
mod tests {
    use crate::agents::agent::*;
    use crate::browser::{BrowserManager, StealthConfig};
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};
    use uuid::Uuid;
    use chrono::Utc;

    #[tokio::test]
    async fn test_agent_creation() {
        let agent = ScrapingAgent::new().expect("Failed to create agent");
        
        assert!(agent.id != Uuid::nil());
        assert_eq!(agent.streamer, None);
        assert!(agent.browser_manager.is_none());
        assert!(agent.browser_instance_id.is_none());
        assert!(agent.message_broadcaster.is_some());
        
        let status = agent.get_status().await;
        assert!(matches!(status, AgentStatus::Idle));
        
        let metrics = agent.get_metrics().await;
        assert_eq!(metrics.messages_scraped, 0);
        assert_eq!(metrics.error_count, 0);
    }

    #[tokio::test]
    async fn test_agent_with_browser_manager() {
        let stealth_config = StealthConfig::default();
        let proxy_list = Vec::new();
        let browser_manager = Arc::new(
            BrowserManager::new(1, stealth_config, proxy_list)
                .await
                .expect("Failed to create browser manager")
        );
        
        let agent = ScrapingAgent::new()
            .expect("Failed to create agent")
            .with_browser_manager(browser_manager.clone());
        
        assert!(agent.browser_manager.is_some());
        
        // verify the browser manager is the same instance
        let agent_browser_manager = agent.browser_manager.as_ref().unwrap();
        assert!(Arc::ptr_eq(agent_browser_manager, &browser_manager));
    }

    #[tokio::test]
    async fn test_agent_status_transitions() {
        let agent = ScrapingAgent::new().expect("Failed to create agent");
        
        // test initial status
        let status = agent.get_status().await;
        assert!(matches!(status, AgentStatus::Idle));
        
        // test status change
        agent.set_status(AgentStatus::Starting).await;
        let status = agent.get_status().await;
        assert!(matches!(status, AgentStatus::Starting));
        
        agent.set_status(AgentStatus::Running).await;
        let status = agent.get_status().await;
        assert!(matches!(status, AgentStatus::Running));
        
        agent.set_status(AgentStatus::Stopped).await;
        let status = agent.get_status().await;
        assert!(matches!(status, AgentStatus::Stopped));
    }

    #[tokio::test]
    async fn test_message_stream() {
        let agent = ScrapingAgent::new().expect("Failed to create agent");
        let mut message_stream = agent.message_stream();
        
        // get the broadcaster to send a test message
        if let Some(broadcaster) = &agent.message_broadcaster {
            let test_message = crate::parser::chat_message::ChatMessage::new(
                "teststreamer".to_string(),
                Utc::now(),
                crate::parser::chat_message::ChatUser {
                    username: "testuser".to_string(),
                    display_name: "testuser".to_string(),
                    color: Some("#FF0000".to_string()),
                    badges: vec!["subscriber".to_string()],
                },
                crate::parser::chat_message::MessageContent {
                    text: "Hello, world!".to_string(),
                    emotes: vec![],
                    fragments: vec![crate::parser::chat_message::MessageFragment {
                        fragment_type: "text".to_string(),
                        content: "Hello, world!".to_string(),
                    }],
                },
                crate::parser::chat_message::StreamContext::default(),
            );
            
            // send the message
            broadcaster.send(test_message.clone()).expect("Failed to send message");
            
            // receive the message with timeout
            let received_message = timeout(Duration::from_millis(100), message_stream.recv())
                .await
                .expect("Timeout waiting for message")
                .expect("Failed to receive message");
            
            assert_eq!(received_message.id, test_message.id);
            assert_eq!(received_message.user.username, test_message.user.username);
            assert_eq!(received_message.message.text, test_message.message.text);
            assert_eq!(received_message.streamer, test_message.streamer);
        }
    }

    #[tokio::test]
    async fn test_metrics_updates() {
        let agent = ScrapingAgent::new().expect("Failed to create agent");
        
        // test initial metrics
        let metrics = agent.get_metrics().await;
        assert_eq!(metrics.messages_scraped, 0);
        assert_eq!(metrics.error_count, 0);
        assert!(metrics.last_message_time.is_none());
        
        // test error count increment
        agent.increment_error_count().await;
        let metrics = agent.get_metrics().await;
        assert_eq!(metrics.error_count, 1);
        
        // test message metrics update
        agent.update_message_metrics(5).await;
        let metrics = agent.get_metrics().await;
        assert_eq!(metrics.messages_scraped, 5);
        assert!(metrics.last_message_time.is_some());
    }

    #[tokio::test]
    async fn test_uptime_calculation() {
        let mut agent = ScrapingAgent::new().expect("Failed to create agent");
        
        // set start time
        agent.start_time = Some(Utc::now() - chrono::Duration::seconds(10));
        
        // update uptime
        agent.update_uptime().await;
        
        let metrics = agent.get_metrics().await;
        assert!(metrics.uptime.as_secs() >= 9); // Should be around 10 seconds, allowing for some variance
        assert!(metrics.uptime.as_secs() <= 11);
    }

    #[tokio::test]
    async fn test_browser_initialization_without_manager() {
        let mut agent = ScrapingAgent::new().expect("Failed to create agent");
        
        // try to initialize browser without browser manager
        let result = agent.initialize_browser().await;
        assert!(result.is_err());
        
        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("No browser manager available"));
        }
    }

    #[tokio::test]
    async fn test_cleanup_browser_without_instance() {
        let mut agent = ScrapingAgent::new().expect("Failed to create agent");
        
        // try to cleanup browser without browser instance
        let result = agent.cleanup_browser().await;
        assert!(result.is_ok()); // Should succeed even without instance
    }
}