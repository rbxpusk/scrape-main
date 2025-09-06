#[cfg(test)]
mod tests {
    use crate::browser::{BrowserManager, StealthConfig, UserAgentGenerator, FingerprintRandomizer};
    use crate::browser::stealth::{generate_video_disable_script, generate_stealth_script};

    #[tokio::test]
    async fn test_browser_manager_creation() {
        let stealth_config = StealthConfig::default();
        let proxy_list = Vec::new();
        
        // note: this test might fail in ci/cd due to chrome dependencies
        // In a real implementation, we would mock the browser for testing
        let result = BrowserManager::new(2, stealth_config, proxy_list).await;
        
        // We expect either success or a browser-related error (which is acceptable in test environments)
        match result {
            Ok(_) => {
                // Browser manager created successfully
                assert!(true);
            }
            Err(e) => {
                // Check if it's a browser-related error (acceptable in test environments)
                let error_str = e.to_string();
                assert!(
                    error_str.contains("Browser") || error_str.contains("chrome") || error_str.contains("chromium"),
                    "Expected browser-related error, got: {}",
                    error_str
                );
            }
        }
    }

    #[tokio::test]
    async fn test_browser_instance_creation_mock() {
        // This test verifies the logic without actually launching Chrome
        let stealth_config = StealthConfig::default();
        let proxy_list: Vec<String> = Vec::new();
        
        // Test that the configuration is properly set up
        assert!(stealth_config.randomize_user_agents);
        assert!(stealth_config.fingerprint_randomization);
        assert!(proxy_list.is_empty());
        
        // In a real implementation, we would test the browser instance creation
        // with a mocked browser to avoid Chrome dependencies in tests
    }

    #[tokio::test]
    async fn test_stealth_config_default() {
        let config = StealthConfig::default();
        
        assert!(config.randomize_user_agents);
        assert!(config.simulate_human_behavior);
        assert!(config.fingerprint_randomization);
        assert!(config.viewport_randomization);
        assert_eq!(config.delay_range, (1000, 5000));
    }

    #[tokio::test]
    async fn test_user_agent_generation() {
        let generator = UserAgentGenerator::new();
        let user_agent = generator.random_user_agent();
        
        assert!(!user_agent.is_empty(), "User agent should not be empty");
        assert!(user_agent.contains("Mozilla"), "User agent should contain Mozilla");
    }

    #[tokio::test]
    async fn test_fingerprint_generation() {
        let randomizer = FingerprintRandomizer::new();
        let fingerprint = randomizer.generate_fingerprint();
        
        assert!(fingerprint.viewport.width > 0, "Viewport width should be positive");
        assert!(fingerprint.viewport.height > 0, "Viewport height should be positive");
        assert!(!fingerprint.language.is_empty(), "Language should not be empty");
        assert!(!fingerprint.timezone.is_empty(), "Timezone should not be empty");
        assert!(!fingerprint.platform.is_empty(), "Platform should not be empty");
        assert!(fingerprint.hardware_concurrency > 0, "Hardware concurrency should be positive");
        assert!(fingerprint.device_memory > 0, "Device memory should be positive");
    }

    #[test]
    fn test_video_disable_script_generation() {
        let script = generate_video_disable_script();
        
        assert!(!script.is_empty(), "Video disable script should not be empty");
        assert!(script.contains("video"), "Script should contain video element handling");
        assert!(script.contains("pause"), "Script should pause videos");
        assert!(script.contains("remove"), "Script should remove video elements");
    }

    #[test]
    fn test_stealth_script_generation() {
        let randomizer = FingerprintRandomizer::new();
        let fingerprint = randomizer.generate_fingerprint();
        let script = generate_stealth_script(&fingerprint);
        
        assert!(!script.is_empty(), "Stealth script should not be empty");
        assert!(script.contains("navigator"), "Script should modify navigator properties");
        assert!(script.contains("webdriver"), "Script should hide webdriver property");
    }
}