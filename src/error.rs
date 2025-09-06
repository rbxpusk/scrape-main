use thiserror::Error;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Error, Debug)]
pub enum ScrapingError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Browser error: {0}")]
    BrowserError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Resource limit reached: {0}")]
    ResourceLimit(String),

    #[error("Agent error: {0}")]
    AgentError(String),

    #[error("TUI error: {0}")]
    TUIError(String),
}

#[derive(Debug)]
pub enum RecoveryStrategy {
    RetryWithBackoff,
    RestartBrowser,
    LogAndContinue,
    SwitchStorage,
    ReloadConfig,
    StopAgent,
}

impl ScrapingError {
    pub fn recovery_strategy(&self) -> RecoveryStrategy {
        match self {
            ScrapingError::NetworkError(_) => RecoveryStrategy::RetryWithBackoff,
            ScrapingError::BrowserError(_) => RecoveryStrategy::RestartBrowser,
            ScrapingError::ParseError(_) => RecoveryStrategy::LogAndContinue,
            ScrapingError::StorageError(_) => RecoveryStrategy::SwitchStorage,
            ScrapingError::ConfigError(_) => RecoveryStrategy::ReloadConfig,
            ScrapingError::ResourceLimit(_) => RecoveryStrategy::StopAgent,
            ScrapingError::AgentError(_) => RecoveryStrategy::RestartBrowser,
            ScrapingError::TUIError(_) => RecoveryStrategy::LogAndContinue,
        }
    }
}

// Conversion implementations for common error types
impl From<std::io::Error> for ScrapingError {
    fn from(err: std::io::Error) -> Self {
        ScrapingError::StorageError(err.to_string())
    }
}

impl From<serde_json::Error> for ScrapingError {
    fn from(err: serde_json::Error) -> Self {
        ScrapingError::ParseError(err.to_string())
    }
}

impl From<toml::de::Error> for ScrapingError {
    fn from(err: toml::de::Error) -> Self {
        ScrapingError::ConfigError(err.to_string())
    }
}

impl From<reqwest::Error> for ScrapingError {
    fn from(err: reqwest::Error) -> Self {
        ScrapingError::NetworkError(err.to_string())
    }
}

impl From<chromiumoxide::error::CdpError> for ScrapingError {
    fn from(err: chromiumoxide::error::CdpError) -> Self {
        ScrapingError::BrowserError(err.to_string())
    }
}