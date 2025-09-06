pub mod discord;

use crate::error::Result;
use crate::parser::ChatMessage;


#[async_trait::async_trait]
pub trait WebhookProvider: Send + Sync {
    async fn send_message(&self, message: &ChatMessage) -> Result<()>;
    async fn send_alert(&self, level: &str, title: &str, message: &str) -> Result<()>;
}

pub struct WebhookManager {
    providers: Vec<Box<dyn WebhookProvider>>,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn WebhookProvider>) {
        self.providers.push(provider);
    }

    pub async fn send_message(&self, message: &ChatMessage) -> Result<()> {
        for provider in &self.providers {
            if let Err(e) = provider.send_message(message).await {
                tracing::warn!("Webhook provider failed to send message: {}", e);
            }
        }
        Ok(())
    }

    pub async fn send_alert(&self, level: &str, title: &str, message: &str) -> Result<()> {
        for provider in &self.providers {
            if let Err(e) = provider.send_alert(level, title, message).await {
                tracing::warn!("Webhook provider failed to send alert: {}", e);
            }
        }
        Ok(())
    }
}