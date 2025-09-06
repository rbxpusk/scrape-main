use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use std::time::Duration;
use notify::{Watcher, RecursiveMode, Event, EventKind};
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

use crate::error::{Result, ScrapingError};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub streamers: Vec<String>,
    pub agents: AgentConfig,
    pub output: OutputConfig,
    pub monitoring: MonitorConfig,
    pub stealth: StealthConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    pub max_concurrent: usize,
    pub retry_attempts: u32,
    pub delay_range: (u64, u64), // milliseconds
    pub proxy_list: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    pub format: String, // "json", "csv", "custom"
    pub directory: PathBuf,
    pub rotation_size: String, // "100MB"
    pub rotation_time: String, // "1h"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitorConfig {
    pub tui_enabled: bool,
    pub api_port: u16,
    pub dashboard_port: Option<u16>,
    pub api_token: Option<String>,
    pub webhook_url: Option<String>,
    pub discord_webhook_url: Option<String>,
    pub custom_css: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StealthConfig {
    pub randomize_user_agents: bool,
    pub simulate_human_behavior: bool,
    pub proxy_rotation: bool,
    pub fingerprint_randomization: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            streamers: vec!["shroud".to_string(), "ninja".to_string()],
            agents: AgentConfig {
                max_concurrent: 5,
                retry_attempts: 3,
                delay_range: (1000, 5000),
                proxy_list: None,
            },
            output: OutputConfig {
                format: "json".to_string(),
                directory: PathBuf::from("./scraped_data"),
                rotation_size: "100MB".to_string(),
                rotation_time: "1h".to_string(),
            },
            monitoring: MonitorConfig {
                tui_enabled: true,
                api_port: 8080,
                dashboard_port: Some(8888),
                api_token: None,
                webhook_url: None,
                discord_webhook_url: None,
                custom_css: None,
            },
            stealth: StealthConfig {
                randomize_user_agents: true,
                simulate_human_behavior: true,
                proxy_rotation: false,
                fingerprint_randomization: true,
            },
        }
    }
}

#[async_trait::async_trait]
pub trait ConfigManager {
    async fn load_config(&self) -> Result<Config>;
    async fn save_config(&self, config: &Config) -> Result<()>;
    async fn watch_config_changes(&self) -> Result<tokio::sync::mpsc::Receiver<Config>>;
    fn validate_config(&self, config: &Config) -> Result<()>;
}

pub struct FileConfigManager {
    config_path: PathBuf,
}

impl FileConfigManager {
    pub fn new(config_path: PathBuf) -> Self {
        Self { config_path }
    }
}

#[async_trait::async_trait]
impl ConfigManager for FileConfigManager {
    async fn load_config(&self) -> Result<Config> {
        info!("Loading configuration from {:?}", self.config_path);
        
        // check if config file exists, create default if not
        if !self.config_path.exists() {
            warn!("Configuration file not found, creating default config at {:?}", self.config_path);
            self.create_default_config().await?;
        }

        // read and parse the config file
        let config_content = fs::read_to_string(&self.config_path)
            .map_err(|e| ScrapingError::ConfigError(format!("Failed to read config file: {}", e)))?;

        let config: Config = toml::from_str(&config_content)
            .map_err(|e| ScrapingError::ConfigError(format!("Failed to parse TOML config: {}", e)))?;

        // validate the loaded config
        self.validate_config(&config)?;

        info!("Configuration loaded successfully");
        Ok(config)
    }

    async fn watch_config_changes(&self) -> Result<tokio::sync::mpsc::Receiver<Config>> {
        let (tx, rx) = mpsc::channel(10);
        let config_path = self.config_path.clone();
        let config_manager = FileConfigManager::new(config_path.clone());

        tokio::spawn(async move {
            if let Err(e) = Self::watch_config_file(config_path, tx, config_manager).await {
                error!("Configuration file watcher error: {}", e);
            }
        });

        Ok(rx)
    }

    fn validate_config(&self, config: &Config) -> Result<()> {
        debug!("Validating configuration");

        // checking streamers list
        if config.streamers.is_empty() {
            return Err(ScrapingError::ConfigError("Streamers list cannot be empty".to_string()).into());
        }

        for streamer in &config.streamers {
            if streamer.trim().is_empty() {
                return Err(ScrapingError::ConfigError("Streamer name cannot be empty".to_string()).into());
            }
            if streamer.contains(' ') {
                return Err(ScrapingError::ConfigError(format!("Streamer name '{}' cannot contain spaces", streamer)).into());
            }
            if streamer.len() > 25 {
                return Err(ScrapingError::ConfigError(format!("Streamer name '{}' is too long (max 25 characters)", streamer)).into());
            }
        }

        // checking agent config
        if config.agents.max_concurrent == 0 {
            return Err(ScrapingError::ConfigError("max_concurrent must be greater than 0".to_string()).into());
        }
        if config.agents.max_concurrent > 50 {
            return Err(ScrapingError::ConfigError("max_concurrent cannot exceed 50 for resource safety".to_string()).into());
        }
        if config.agents.retry_attempts > 10 {
            return Err(ScrapingError::ConfigError("retry_attempts cannot exceed 10".to_string()).into());
        }
        if config.agents.delay_range.0 >= config.agents.delay_range.1 {
            return Err(ScrapingError::ConfigError("delay_range minimum must be less than maximum".to_string()).into());
        }
        if config.agents.delay_range.1 > 60000 {
            return Err(ScrapingError::ConfigError("delay_range maximum cannot exceed 60 seconds".to_string()).into());
        }

        // checking proxy list if provided
        if let Some(ref proxies) = config.agents.proxy_list {
            for proxy in proxies {
                if !proxy.contains(':') {
                    return Err(ScrapingError::ConfigError(format!("Invalid proxy format '{}', expected 'host:port'", proxy)).into());
                }
            }
        }

        // checking output config
        let valid_formats = ["json", "csv", "custom"];
        if !valid_formats.contains(&config.output.format.as_str()) {
            return Err(ScrapingError::ConfigError(format!("Invalid output format '{}', must be one of: {:?}", config.output.format, valid_formats)).into());
        }

        // Validate rotation size format
        if !Self::is_valid_size_format(&config.output.rotation_size) {
            return Err(ScrapingError::ConfigError(format!("Invalid rotation_size format '{}', expected format like '100MB', '1GB'", config.output.rotation_size)).into());
        }

        // Validate rotation time format
        if !Self::is_valid_time_format(&config.output.rotation_time) {
            return Err(ScrapingError::ConfigError(format!("Invalid rotation_time format '{}', expected format like '1h', '30m', '1d'", config.output.rotation_time)).into());
        }

        // checking monitoring config
        if config.monitoring.api_port < 1024 {
            return Err(ScrapingError::ConfigError("api_port must be between 1024 and 65535".to_string()).into());
        }

        // Validate webhook URL if provided
        if let Some(ref webhook_url) = config.monitoring.webhook_url {
            if !webhook_url.starts_with("http://") && !webhook_url.starts_with("https://") {
                return Err(ScrapingError::ConfigError("webhook_url must start with http:// or https://".to_string()).into());
            }
        }

        // Validate custom CSS file if provided
        if let Some(ref css_path) = config.monitoring.custom_css {
            if !css_path.exists() {
                return Err(ScrapingError::ConfigError(format!("Custom CSS file not found: {:?}", css_path)).into());
            }
        }

        debug!("Configuration validation passed");
        Ok(())
    }

    async fn save_config(&self, config: &Config) -> Result<()> {
        info!("Saving configuration to {:?}", self.config_path);
        
        let toml_content = toml::to_string_pretty(config)
            .map_err(|e| ScrapingError::ConfigError(format!("Failed to serialize config: {}", e)))?;
        
        fs::write(&self.config_path, toml_content)
            .map_err(|e| ScrapingError::ConfigError(format!("Failed to write config file: {}", e)))?;
        
        info!("Configuration saved successfully");
        Ok(())
    }}
impl FileConfigManager {
    /// Create a default configuration file
    async fn create_default_config(&self) -> Result<()> {
        let default_config = Config::default();
        let toml_content = toml::to_string_pretty(&default_config)
            .map_err(|e| ScrapingError::ConfigError(format!("Failed to serialize default config: {}", e)))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ScrapingError::ConfigError(format!("Failed to create config directory: {}", e)))?;
        }

        fs::write(&self.config_path, toml_content)
            .map_err(|e| ScrapingError::ConfigError(format!("Failed to write default config: {}", e)))?;

        info!("Default configuration file created at {:?}", self.config_path);
        Ok(())
    }

    /// Watch configuration file for changes and send updates through the channel
    async fn watch_config_file(
        config_path: PathBuf,
        tx: mpsc::Sender<Config>,
        config_manager: FileConfigManager,
    ) -> Result<()> {
        let (file_tx, mut file_rx) = mpsc::channel(100);

        // Set up file system watcher
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    if let Err(e) = file_tx.blocking_send(event) {
                        error!("Failed to send file system event: {}", e);
                    }
                }
                Err(e) => error!("File system watcher error: {}", e),
            }
        }).map_err(|e| ScrapingError::ConfigError(format!("Failed to create file watcher: {}", e)))?;

        // Watch the config file's parent directory
        let watch_path = config_path.parent().unwrap_or(&config_path);
        watcher.watch(watch_path, RecursiveMode::NonRecursive)
            .map_err(|e| ScrapingError::ConfigError(format!("Failed to watch config directory: {}", e)))?;

        info!("Started watching configuration file: {:?}", config_path);

        // Process file system events
        while let Some(event) = file_rx.recv().await {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    // Check if the event is for our config file
                    if event.paths.iter().any(|p| p == &config_path) {
                        debug!("Configuration file changed, reloading...");
                        
                        // Add a small delay to ensure file write is complete
                        tokio::time::sleep(Duration::from_millis(100)).await;

                        match config_manager.load_config().await {
                            Ok(new_config) => {
                                info!("Configuration reloaded successfully");
                                if let Err(e) = tx.send(new_config).await {
                                    error!("Failed to send updated config: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Failed to reload configuration: {}", e);
                                // Continue watching even if reload fails
                            }
                        }
                    }
                }
                _ => {} // Ignore other event types
            }
        }

        Ok(())
    }

    /// Validate size format (e.g., "100MB", "1GB")
    fn is_valid_size_format(size_str: &str) -> bool {
        let size_str = size_str.to_uppercase();
        let valid_suffixes = ["B", "KB", "MB", "GB", "TB"];
        
        for suffix in &valid_suffixes {
            if size_str.ends_with(suffix) {
                let number_part = &size_str[..size_str.len() - suffix.len()];
                if let Ok(_) = number_part.parse::<u64>() {
                    return true;
                }
            }
        }
        false
    }

    /// Validate time format (e.g., "1h", "30m", "1d")
    fn is_valid_time_format(time_str: &str) -> bool {
        let time_str = time_str.to_lowercase();
        let valid_suffixes = ["s", "m", "h", "d"];
        
        for suffix in &valid_suffixes {
            if time_str.ends_with(suffix) {
                let number_part = &time_str[..time_str.len() - suffix.len()];
                if let Ok(_) = number_part.parse::<u64>() {
                    return true;
                }
            }
        }
        false
    }

    /// Parse size string to bytes
    pub fn parse_size_to_bytes(size_str: &str) -> Result<u64> {
        let size_str = size_str.to_uppercase();
        // Order matters - check longer suffixes first to avoid partial matches
        let multipliers = [
            ("TB", 1024_u64.pow(4)),
            ("GB", 1024 * 1024 * 1024),
            ("MB", 1024 * 1024),
            ("KB", 1024),
            ("B", 1),
        ];

        for (suffix, multiplier) in &multipliers {
            if size_str.ends_with(suffix) {
                let number_part = &size_str[..size_str.len() - suffix.len()];
                let number: u64 = number_part.parse()
                    .map_err(|_| ScrapingError::ConfigError(format!("Invalid number in size format: {}", size_str)))?;
                return Ok(number * multiplier);
            }
        }

        Err(ScrapingError::ConfigError(format!("Invalid size format: {}", size_str)).into())
    }

    /// Parse time string to duration
    pub fn parse_time_to_duration(time_str: &str) -> Result<Duration> {
        let time_str = time_str.to_lowercase();
        let multipliers = [
            ("s", 1),
            ("m", 60),
            ("h", 3600),
            ("d", 86400),
        ];

        for (suffix, multiplier) in &multipliers {
            if time_str.ends_with(suffix) {
                let number_part = &time_str[..time_str.len() - suffix.len()];
                let number: u64 = number_part.parse()
                    .map_err(|_| ScrapingError::ConfigError(format!("Invalid time format: {}", time_str)))?;
                return Ok(Duration::from_secs(number * multiplier));
            }
        }

        Err(ScrapingError::ConfigError(format!("Invalid time format: {}", time_str)).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_load_default_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let manager = FileConfigManager::new(config_path.clone());

        let config = manager.load_config().await.unwrap();
        
        assert_eq!(config.streamers, vec!["shroud", "ninja"]);
        assert_eq!(config.agents.max_concurrent, 5);
        assert_eq!(config.output.format, "json");
        assert!(config_path.exists());
    }

    #[tokio::test]
    async fn test_config_validation() {
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

    #[test]
    fn test_size_format_validation() {
        assert!(FileConfigManager::is_valid_size_format("100MB"));
        assert!(FileConfigManager::is_valid_size_format("1GB"));
        assert!(FileConfigManager::is_valid_size_format("500kb"));
        assert!(!FileConfigManager::is_valid_size_format("invalid"));
        assert!(!FileConfigManager::is_valid_size_format("100"));
    }

    #[test]
    fn test_time_format_validation() {
        assert!(FileConfigManager::is_valid_time_format("1h"));
        assert!(FileConfigManager::is_valid_time_format("30m"));
        assert!(FileConfigManager::is_valid_time_format("1d"));
        assert!(!FileConfigManager::is_valid_time_format("invalid"));
        assert!(!FileConfigManager::is_valid_time_format("100"));
    }

    #[test]
    fn test_parse_size_to_bytes() {
        assert_eq!(FileConfigManager::parse_size_to_bytes("100MB").unwrap(), 100 * 1024 * 1024);
        assert_eq!(FileConfigManager::parse_size_to_bytes("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(FileConfigManager::parse_size_to_bytes("500KB").unwrap(), 500 * 1024);
        assert!(FileConfigManager::parse_size_to_bytes("invalid").is_err());
    }

    #[test]
    fn test_parse_time_to_duration() {
        assert_eq!(FileConfigManager::parse_time_to_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(FileConfigManager::parse_time_to_duration("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(FileConfigManager::parse_time_to_duration("1d").unwrap(), Duration::from_secs(86400));
        assert!(FileConfigManager::parse_time_to_duration("invalid").is_err());
    }
}