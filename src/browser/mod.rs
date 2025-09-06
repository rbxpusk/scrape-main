pub mod manager;
pub mod stealth;

#[cfg(test)]
mod tests;

pub use manager::{BrowserManager, BrowserPool, BrowserInstance, BrowserInstanceId};
pub use stealth::{StealthConfig, UserAgentGenerator, FingerprintRandomizer};