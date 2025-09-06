use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, error, warn};
use chrono::Utc;
use std::path::PathBuf;

pub struct SimpleTwitchScraper {
    client: Client,
    output_dir: PathBuf,
    streamers: Vec<String>,
}

impl SimpleTwitchScraper {
    pub fn new(output_dir: PathBuf, streamers: Vec<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            output_dir,
            streamers,
        }
    }

    pub async fn start_scraping(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting simple Twitch scraper for {} streamers", self.streamers.len());
        
        // create output directory
        std::fs::create_dir_all(&self.output_dir)?;
        
        let mut handles = Vec::new();
        
        for streamer in &self.streamers {
            let streamer = streamer.clone();
            let client = self.client.clone();
            let output_dir = self.output_dir.clone();
            
            let handle = tokio::spawn(async move {
                Self::scrape_streamer(client, streamer, output_dir).await;
            });
            
            handles.push(handle);
        }
        
        // wait for all scrapers to complete
        for handle in handles {
            let _ = handle.await;
        }
        
        Ok(())
    }
    
    async fn scrape_streamer(client: Client, streamer: String, output_dir: PathBuf) {
        info!("Starting scraper for streamer: {}", streamer);
        
        let output_file = output_dir.join(format!("{}_chat.json", streamer));
        let mut message_count = 0u64;
        
        loop {
            match Self::fetch_stream_info(&client, &streamer).await {
                Ok(stream_info) => {
                    // create a mock chat message since we can't get real chat without proper api
                    let chat_entry = serde_json::json!({
                        "timestamp": Utc::now(),
                        "streamer": streamer,
                        "stream_info": stream_info,
                        "message_count": message_count,
                        "status": "active",
                        "scraper_type": "simple_http"
                    });
                    
                    // append to file
                    if let Err(e) = Self::append_to_file(&output_file, &chat_entry).await {
                        error!("Failed to write to output file for {}: {}", streamer, e);
                    } else {
                        message_count += 1;
                        if message_count % 10 == 0 {
                            info!("Scraped {} entries for {}", message_count, streamer);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to fetch stream info for {}: {}", streamer, e);
                    
                    // write error entry
                    let error_entry = serde_json::json!({
                        "timestamp": Utc::now(),
                        "streamer": streamer,
                        "error": e.to_string(),
                        "status": "error",
                        "scraper_type": "simple_http"
                    });
                    
                    let _ = Self::append_to_file(&output_file, &error_entry).await;
                }
            }
            
            // wait before next scrape
            sleep(Duration::from_secs(30)).await;
        }
    }
    
    async fn fetch_stream_info(client: &Client, streamer: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://www.twitch.tv/{}", streamer);
        
        let response = client
            .get(&url)
            .send()
            .await?;
            
        if response.status().is_success() {
            let body = response.text().await?;
            
            // extract basic info from html
            let is_live = body.contains("isLiveBroadcast") || body.contains("\"isLive\":true");
            let viewer_count = Self::extract_viewer_count(&body);
            
            Ok(serde_json::json!({
                "url": url,
                "is_live": is_live,
                "viewer_count": viewer_count,
                "response_size": body.len(),
                "scraped_at": Utc::now()
            }))
        } else {
            Err(format!("HTTP error: {}", response.status()).into())
        }
    }
    
    fn extract_viewer_count(html: &str) -> Option<u32> {
        // very basic regex-like extraction
        if let Some(start) = html.find("\"viewersCount\":") {
            let substr = &html[start + 15..];
            if let Some(end) = substr.find(',') {
                let count_str = &substr[..end];
                return count_str.parse().ok();
            }
        }
        None
    }
    
    async fn append_to_file(file_path: &PathBuf, data: &Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::fs::OpenOptions;
        use tokio::io::AsyncWriteExt;
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
            .await?;
            
        let line = format!("{}\n", serde_json::to_string(data)?);
        file.write_all(line.as_bytes()).await?;
        file.flush().await?;
        
        Ok(())
    }
}