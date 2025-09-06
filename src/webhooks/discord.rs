use crate::error::{Result, ScrapingError};
use crate::parser::ChatMessage;
use crate::webhooks::WebhookProvider;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

pub struct DiscordWebhook {
    client: Client,
    webhook_url: String,
    rate_limiter: tokio::sync::Semaphore,
}

impl DiscordWebhook {
    pub fn new(webhook_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| Box::new(ScrapingError::NetworkError(format!("Failed to create HTTP client: {}", e))) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(Self {
            client,
            webhook_url,
            rate_limiter: tokio::sync::Semaphore::new(5), // Discord allows 5 requests per 2 seconds
        })
    }

    async fn send_webhook(&self, payload: Value) -> Result<()> {
        let _permit = self.rate_limiter.acquire().await
            .map_err(|e| Box::new(ScrapingError::NetworkError(format!("Rate limiter error: {}", e))) as Box<dyn std::error::Error + Send + Sync>)?;

        let response = self.client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| Box::new(ScrapingError::NetworkError(format!("Failed to send webhook: {}", e))) as Box<dyn std::error::Error + Send + Sync>)?;

        if response.status().is_success() {
            debug!("Discord webhook sent successfully");
        } else if response.status().as_u16() == 429 {
            // Rate limited, wait and retry
            warn!("Discord webhook rate limited, waiting...");
            sleep(Duration::from_secs(2)).await;
            return Box::pin(self.send_webhook(payload)).await;
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Box::new(ScrapingError::NetworkError(format!(
                "Discord webhook failed with status {}: {}",
                status, body
            ))));
        }

        // respect discord rate limits
        sleep(Duration::from_millis(400)).await;
        Ok(())
    }

    fn create_chat_embed(&self, message: &ChatMessage) -> Value {
        let color = self.parse_user_color(&message.user.color);
        
        json!({
            "embeds": [{
                "title": format!("💬 Chat from {}", message.streamer),
                "color": color,
                "fields": [
                    {
                        "name": "👤 User",
                        "value": format!("**{}**", message.user.display_name),
                        "inline": true
                    },
                    {
                        "name": "💭 Message",
                        "value": message.message.text,
                        "inline": false
                    },
                    {
                        "name": "📺 Channel",
                        "value": format!("twitch.tv/{}", message.streamer),
                        "inline": true
                    }
                ],
                "timestamp": message.timestamp.to_rfc3339(),
                "footer": {
                    "text": "Twitch Chat Scraper",
                    "icon_url": "https://static-cdn.jtvnw.net/jtv_user_pictures/8a6381c7-d0c0-4576-b179-38bd5ce1d6af-profile_image-70x70.png"
                }
            }]
        })
    }

    fn create_alert_embed(&self, level: &str, title: &str, message: &str) -> Value {
        let (color, emoji) = match level.to_lowercase().as_str() {
            "critical" => (0xFF0000, "🚨"), // Red
            "warning" => (0xFFFF00, "⚠️"),  // Yellow
            "info" => (0x0099FF, "ℹ️"),     // Blue
            _ => (0x808080, "📢"),          // Gray
        };

        json!({
            "embeds": [{
                "title": format!("{} {}", emoji, title),
                "description": message,
                "color": color,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "footer": {
                    "text": "Twitch Chat Scraper Alert",
                    "icon_url": "https://static-cdn.jtvnw.net/jtv_user_pictures/8a6381c7-d0c0-4576-b179-38bd5ce1d6af-profile_image-70x70.png"
                }
            }]
        })
    }

    fn parse_user_color(&self, color_str: &Option<String>) -> u32 {
        if let Some(color) = color_str {
            if color.starts_with("rgb(") && color.ends_with(')') {
                let rgb_str = &color[4..color.len()-1];
                let parts: Vec<&str> = rgb_str.split(',').collect();
                if parts.len() == 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].trim().parse::<u32>(),
                        parts[1].trim().parse::<u32>(),
                        parts[2].trim().parse::<u32>(),
                    ) {
                        return (r << 16) | (g << 8) | b;
                    }
                }
            } else if color.starts_with('#') {
                if let Ok(hex) = u32::from_str_radix(&color[1..], 16) {
                    return hex;
                }
            }
        }
        0x9146FF // Default Twitch purple
    }
}

#[async_trait::async_trait]
impl WebhookProvider for DiscordWebhook {
    async fn send_message(&self, message: &ChatMessage) -> Result<()> {
        let payload = self.create_chat_embed(message);
        self.send_webhook(payload).await
    }

    async fn send_alert(&self, level: &str, title: &str, message: &str) -> Result<()> {
        let payload = self.create_alert_embed(level, title, message);
        self.send_webhook(payload).await
    }
}