use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use chromiumoxide::cdp::browser_protocol::emulation::SetUserAgentOverrideParams;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use std::time::Instant;

use crate::browser::stealth::{StealthConfig, UserAgentGenerator, FingerprintRandomizer, BrowserFingerprint, generate_video_disable_script, generate_stealth_script};
use crate::error::{Result, ScrapingError};

pub type BrowserInstanceId = Uuid;

#[derive(Debug, Clone)]
pub struct BrowserInstance {
    pub id: BrowserInstanceId,
    pub page: Page,
    pub fingerprint: BrowserFingerprint,
    pub user_agent: String,
    pub proxy: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl BrowserInstance {
    pub async fn navigate_to_twitch_stream(&self, streamer: &str) -> Result<()> {
        let url = format!("https://www.twitch.tv/{}", streamer);
        info!("Navigating browser instance {} to {}", self.id, url);
        
        self.page
            .goto(&url)
            .await
            .map_err(|e| ScrapingError::BrowserError(format!("Failed to navigate to {}: {}", url, e)))?;

        // Wait for page to load
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Inject video disable script
        self.inject_video_disable_script().await?;
        
        // Inject stealth script
        self.inject_stealth_script().await?;

        debug!("Successfully navigated to {} and injected scripts", url);
        Ok(())
    }

    pub async fn inject_video_disable_script(&self) -> Result<()> {
        let script = generate_video_disable_script();
        
        self.page
            .evaluate(script)
            .await
            .map_err(|e| ScrapingError::BrowserError(format!("Failed to inject video disable script: {}", e)))?;

        debug!("Injected video disable script for browser instance {}", self.id);
        Ok(())
    }

    pub async fn inject_stealth_script(&self) -> Result<()> {
        let script = generate_stealth_script(&self.fingerprint);
        
        self.page
            .evaluate(script.as_str())
            .await
            .map_err(|e| ScrapingError::BrowserError(format!("Failed to inject stealth script: {}", e)))?;

        debug!("Injected stealth script for browser instance {}", self.id);
        Ok(())
    }

    pub async fn get_chat_html(&self) -> Result<String> {
        // Wait for chat to load
        let _chat_selector = "[data-a-target='chat-scroller']";
        
        // Try to find chat element with a timeout
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        
        // Get the chat HTML
        let html = self.page
            .content()
            .await
            .map_err(|e| ScrapingError::BrowserError(format!("Failed to get page content: {}", e)))?;
        
        // Check if chat content is present in the HTML
        if html.contains("chat-scroller") || html.contains("chat-line") {
            Ok(html)
        } else {
            warn!("Chat element not found in HTML for browser instance {}", self.id);
            Err(ScrapingError::BrowserError("Chat element not found in page".to_string()).into())
        }
    }

    pub async fn close(self) -> Result<()> {
        self.page
            .close()
            .await
            .map_err(|e| ScrapingError::BrowserError(format!("Failed to close browser instance: {}", e)))?;
        
        info!("Closed browser instance {}", self.id);
        Ok(())
    }
}

pub struct BrowserPool {
    instances: Arc<RwLock<HashMap<BrowserInstanceId, BrowserInstance>>>,
    browser: Arc<Browser>,
    stealth_config: StealthConfig,
    user_agent_generator: UserAgentGenerator,
    fingerprint_randomizer: FingerprintRandomizer,
    max_instances: usize,
    proxy_list: Vec<String>,
    proxy_index: Arc<Mutex<usize>>,
    bad_proxies: Arc<RwLock<HashMap<String, Instant>>>,
}

impl BrowserPool {
    pub async fn new(max_instances: usize, stealth_config: StealthConfig) -> Result<Self> {
        let browser = Self::create_browser(&stealth_config).await?;
        
        Ok(Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            browser: Arc::new(browser),
            stealth_config,
            user_agent_generator: UserAgentGenerator::new(),
            fingerprint_randomizer: FingerprintRandomizer::new(),
            max_instances,
            proxy_list: vec![],
            proxy_index: Arc::new(Mutex::new(0)),
            bad_proxies: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn report_bad_proxy(&self, proxy: String) {
        let mut bad_proxies = self.bad_proxies.write().await;
        bad_proxies.insert(proxy.clone(), Instant::now());
        warn!("Reported bad proxy: {}", proxy);
    }

    async fn create_browser(stealth_config: &StealthConfig) -> Result<Browser> {
        info!("Creating browser with stealth config: {:?}", stealth_config);
        
        // kill any existing chrome processes that might be hanging
        let _ = std::process::Command::new("pkill")
            .args(&["-f", "chrome"])
            .output();
        
        // create unique user data dir to avoid singleton lock issues
        let user_data_dir = format!("/tmp/twitch-scraper-{}-{}", 
            std::process::id(), 
            uuid::Uuid::new_v4()
        );
        
        // clean up any existing lock files
        let _ = std::fs::remove_dir_all("/tmp/chromiumoxide-runner");
        let _ = std::fs::create_dir_all(&user_data_dir);
        
        // wait a bit for cleanup
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let mut config = BrowserConfig::builder()
            .no_sandbox()
            .args(vec![
                &format!("--user-data-dir={}", user_data_dir),
                "--headless",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-dev-shm-usage",
                "--disable-extensions",
                "--disable-plugins",
                "--disable-images",    // save bandwidth and resources
                "--mute-audio",
                "--no-first-run",
                "--disable-default-apps",
                "--disable-sync",
                "--disable-background-networking",
                "--disable-web-security",    // allow cross-origin requests
                "--disable-features=VizDisplayCompositor",
                "--remote-debugging-port=0",    // use random port
                "--disable-background-timer-throttling",
                "--disable-renderer-backgrounding",
                "--disable-backgrounding-occluded-windows",
                "--disable-blink-features=AutomationControlled",    // hide automation
                "--disable-dev-tools",
                "--disable-logging",
                "--silent",
                "--log-level=3", // Only fatal errors
            ]);

        if stealth_config.fingerprint_randomization {
            config = config.args(vec![
                "--disable-canvas-aa",
                "--disable-2d-canvas-clip-aa",
                "--disable-gl-drawing-for-tests",
            ]);
        }

        let browser_config = config
            .build()
            .map_err(|e| ScrapingError::BrowserError(format!("Failed to create browser config: {}", e)))?;

        info!("Launching browser with config...");
        
        // Retry browser launch up to 3 times
        let mut last_error = None;
        for attempt in 1..=3 {
            match Browser::launch(browser_config.clone()).await {
                Ok((browser, handler)) => {
                    info!("Browser launched successfully on attempt {}", attempt);
                    
                    // Spawn the handler task with better error handling
                    tokio::spawn(async move {
                        let mut handler = handler;
                        while let Some(h) = handler.next().await {
                            if let Err(e) = h {
                                // filter out common websocket deserialization errors
                                let error_msg = e.to_string();
                                if error_msg.contains("data did not match any variant") || 
                                   error_msg.contains("untagged enum Message") {
                                    debug!("Ignoring WebSocket deserialization error: {}", e);
                                } else {
                                    warn!("Browser handler error: {}", e);
                                }
                                // don't break on errors, continue handling
                            }
                        }
                        debug!("Browser handler task ended");
                    });

                    info!("Created new browser instance");
                    return Ok(browser);
                }
                Err(e) => {
                    error!("Browser launch attempt {} failed: {}", attempt, e);
                    last_error = Some(e);
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
        
        Err(ScrapingError::BrowserError(format!(
            "Failed to launch browser after 3 attempts: {}", 
            last_error.unwrap()
        )).into())


    }

    pub async fn create_instance(&self) -> Result<BrowserInstanceId> {
        let instances = self.instances.read().await;
        let current_count = instances.len();
        if current_count >= self.max_instances {
            error!("Cannot create browser instance: {} instances already exist (max: {})", 
                   current_count, self.max_instances);
            return Err(ScrapingError::ResourceLimit(
                format!("Maximum browser instances ({}) reached", self.max_instances)
            ).into());
        }
        info!("Creating browser instance ({}/{})", current_count + 1, self.max_instances);
        drop(instances);

        let instance_id = Uuid::new_v4();
        let fingerprint = self.fingerprint_randomizer.generate_fingerprint();
        let user_agent = self.user_agent_generator.random_user_agent().to_string();
        let proxy = self.get_next_proxy().await;

        // Create new page with retry logic
        info!("Creating new browser page for instance {}", instance_id);
        let page = match tokio::time::timeout(
            Duration::from_secs(10),
            self.browser.new_page("about:blank")
        ).await {
            Ok(Ok(page)) => {
                info!("Successfully created browser page for instance {}", instance_id);
                page
            }
            Ok(Err(e)) => {
                error!("Failed to create new page for instance {}: {}", instance_id, e);
                return Err(ScrapingError::BrowserError(format!("Failed to create new page: {}", e)).into());
            }
            Err(_) => {
                error!("Timeout creating new page for instance {}", instance_id);
                return Err(ScrapingError::BrowserError("Timeout creating new page".to_string()).into());
            }
        };

        // set viewport if randomization enabled
        if self.stealth_config.randomize_user_agents {
            use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
            
            let device_metrics = SetDeviceMetricsOverrideParams::builder()
                .width(fingerprint.viewport.width as i64)
                .height(fingerprint.viewport.height as i64)
                .device_scale_factor(1.0)
                .mobile(false)
                .build()
                .map_err(|e| ScrapingError::BrowserError(format!("Failed to build device metrics: {}", e)))?;

            page.execute(device_metrics)
                .await
                .map_err(|e| ScrapingError::BrowserError(format!("Failed to set viewport: {}", e)))?;
        }

        // set user agent if randomization enabled
        if self.stealth_config.randomize_user_agents {
            let user_agent_params = SetUserAgentOverrideParams::builder()
                .user_agent(&user_agent)
                .accept_language(&fingerprint.language)
                .platform(&fingerprint.platform)
                .build()
                .map_err(|e| ScrapingError::BrowserError(format!("Failed to build user agent params: {}", e)))?;

            page.execute(user_agent_params)
                .await
                .map_err(|e| ScrapingError::BrowserError(format!("Failed to set user agent: {}", e)))?;
        }

        let instance = BrowserInstance {
            id: instance_id,
            page,
            fingerprint,
            user_agent,
            proxy,
            created_at: chrono::Utc::now(),
        };

        let mut instances = self.instances.write().await;
        instances.insert(instance_id, instance);
        drop(instances);

        info!("Created browser instance {} with fingerprint", instance_id);
        Ok(instance_id)
    }

    pub async fn get_instance(&self, instance_id: BrowserInstanceId) -> Option<BrowserInstance> {
        let instances = self.instances.read().await;
        instances.get(&instance_id).cloned()
    }

    pub async fn remove_instance(&self, instance_id: BrowserInstanceId) -> Result<()> {
        let mut instances = self.instances.write().await;
        
        if let Some(instance) = instances.remove(&instance_id) {
            drop(instances);
            instance.close().await?;
            info!("Removed browser instance {}", instance_id);
        }
        
        Ok(())
    }

    pub async fn get_instance_count(&self) -> usize {
        let instances = self.instances.read().await;
        instances.len()
    }

    pub async fn close_all_instances(&self) -> Result<()> {
        let mut instances = self.instances.write().await;
        let instance_ids: Vec<BrowserInstanceId> = instances.keys().cloned().collect();
        
        for instance_id in instance_ids {
            if let Some(instance) = instances.remove(&instance_id) {
                if let Err(e) = instance.close().await {
                    error!("Failed to close browser instance {}: {}", instance_id, e);
                }
            }
        }
        
        info!("Closed all browser instances");
        Ok(())
    }

    async fn get_next_proxy(&self) -> Option<String> {
        if self.proxy_list.is_empty() {
            return None;
        }

        let mut index = self.proxy_index.lock().await;
        let mut bad_proxies = self.bad_proxies.write().await;

        let num_proxies = self.proxy_list.len();
        for _ in 0..num_proxies { // Iterate through all proxies once
            let current_proxy = self.proxy_list[*index].clone();
            *index = (*index + 1) % num_proxies;

            if let Some(reported_time) = bad_proxies.get(&current_proxy) {
                // if proxy still in cooldown (e.g. 5 minutes)
                if reported_time.elapsed() < Duration::from_secs(300) {
                    debug!("Skipping bad proxy {} (still in cooldown)", current_proxy);
                    continue;    // skip this proxy, try next one
                } else {
                    // cooldown expired, remove from bad list
                    bad_proxies.remove(&current_proxy);
                    debug!("Proxy {} cooldown expired, re-enabling", current_proxy);
                }
            }
            // proxy either not bad or cooldown expired
            return Some(current_proxy);
        }

        warn!("All proxies are currently in cooldown or unavailable.");
        None // All proxies are bad or in cooldown
    }

    pub async fn cleanup_old_instances(&self, max_age: chrono::Duration) -> Result<()> {
        let now = chrono::Utc::now();
        let mut instances = self.instances.write().await;
        let mut to_remove = Vec::new();

        for (id, instance) in instances.iter() {
            if now.signed_duration_since(instance.created_at) > max_age {
                to_remove.push(*id);
            }
        }

        let removed_count = to_remove.len();
        for id in to_remove {
            if let Some(instance) = instances.remove(&id) {
                if let Err(e) = instance.close().await {
                    error!("Failed to close old browser instance {}: {}", id, e);
                }
            }
        }

        if removed_count > 0 {
            info!("Cleaned up {} old browser instances", removed_count);
        }

        Ok(())
    }
}

pub struct BrowserManager {
    pool: BrowserPool,
}

impl BrowserManager {
    pub async fn new(max_concurrent_sessions: usize, stealth_config: StealthConfig) -> Result<Self> {
        let pool = BrowserPool::new(max_concurrent_sessions, stealth_config).await?;
        
        Ok(Self { pool })
    }

    pub async fn create_browser_instance(&self) -> Result<BrowserInstanceId> {
        self.pool.create_instance().await
    }

    pub async fn get_browser_instance(&self, instance_id: BrowserInstanceId) -> Option<BrowserInstance> {
        self.pool.get_instance(instance_id).await
    }

    pub async fn remove_browser_instance(&self, instance_id: BrowserInstanceId) -> Result<()> {
        self.pool.remove_instance(instance_id).await
    }

    pub async fn get_active_instance_count(&self) -> usize {
        self.pool.get_instance_count().await
    }

    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down browser manager");
        self.pool.close_all_instances().await
    }

    pub async fn cleanup_old_instances(&self, max_age_hours: u64) -> Result<()> {
        let max_age = chrono::Duration::hours(max_age_hours as i64);
        self.pool.cleanup_old_instances(max_age).await
    }
}

impl Drop for BrowserManager {
    fn drop(&mut self) {
        info!("Browser manager dropped");
    }
}
