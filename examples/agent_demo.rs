use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};
use twitch_chat_scraper::{
    agents::{Agent, ScrapingAgent},
    browser::{BrowserManager, StealthConfig},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    info!("Starting Twitch Chat Scraper Agent Demo");

    // setting up browser manager with stealth
    let stealth_config = StealthConfig::default();
    let proxy_list = Vec::new(); // No proxies for demo
    let browser_manager = Arc::new(
        BrowserManager::new(1, stealth_config, proxy_list).await?
    );

    // creating a scraping agent
    let mut agent = ScrapingAgent::new()?
        .with_browser_manager(browser_manager);

    info!("Created agent with ID: {}", agent.id);

    // getting initial status and metrics
    let status = agent.get_status().await;
    let metrics = agent.get_metrics().await;
    info!("Initial status: {:?}", status);
    info!("Initial metrics: {:?}", metrics);

    // setting up stream to get messages
    let mut message_stream = agent.message_stream();

    // starting a task to listen for messages
    let message_listener = tokio::spawn(async move {
        let mut message_count = 0;
        while let Some(message) = message_stream.recv().await {
            message_count += 1;
            info!("Received message #{}: {} from {}: {}", 
                  message_count, message.id, message.username, message.message);
            
            // stopping after 10 messages for demo
            if message_count >= 10 {
                info!("Received 10 messages, stopping listener");
                break;
            }
        }
    });

    // note: in real use, start with actual streamer
    // for demo, just showing lifecycle without connecting
    info!("Agent demo completed successfully");
    
    // simulating some activity
    agent.set_status(twitch_chat_scraper::agents::AgentStatus::Starting).await;
    sleep(Duration::from_millis(100)).await;
    
    agent.set_status(twitch_chat_scraper::agents::AgentStatus::Running).await;
    sleep(Duration::from_millis(100)).await;
    
    // Update some metrics to show functionality
    agent.update_message_metrics(5).await;
    agent.increment_error_count().await;
    
    let final_status = agent.get_status().await;
    let final_metrics = agent.get_metrics().await;
    info!("Final status: {:?}", final_status);
    info!("Final metrics: {:?}", final_metrics);

    // stopping the agent
    agent.set_status(twitch_chat_scraper::agents::AgentStatus::Stopped).await;
    
    // canceling the listener task
    message_listener.abort();
    
    info!("Agent demo completed successfully");
    Ok(())
}