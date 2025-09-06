use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthConfig {
    pub randomize_user_agents: bool,
    pub simulate_human_behavior: bool,
    pub proxy_rotation: bool,
    pub fingerprint_randomization: bool,
    pub viewport_randomization: bool,
    pub delay_range: (u64, u64), // milliseconds
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self {
            randomize_user_agents: true,
            simulate_human_behavior: true,
            proxy_rotation: false,
            fingerprint_randomization: true,
            viewport_randomization: true,
            delay_range: (1000, 5000),
        }
    }
}

pub struct UserAgentGenerator {
    user_agents: Vec<String>,
}

impl UserAgentGenerator {
    pub fn new() -> Self {
        let user_agents = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36".to_string(),
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
        ];

        Self { user_agents }
    }

    pub fn random_user_agent(&self) -> &str {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.user_agents.len());
        &self.user_agents[index]
    }
}

#[derive(Debug, Clone)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
}

pub struct FingerprintRandomizer {
    viewports: Vec<ViewportSize>,
    languages: Vec<String>,
    timezones: Vec<String>,
}

impl FingerprintRandomizer {
    pub fn new() -> Self {
        let viewports = vec![
            ViewportSize { width: 1920, height: 1080 },
            ViewportSize { width: 1366, height: 768 },
            ViewportSize { width: 1536, height: 864 },
            ViewportSize { width: 1440, height: 900 },
            ViewportSize { width: 1280, height: 720 },
            ViewportSize { width: 1600, height: 900 },
            ViewportSize { width: 2560, height: 1440 },
        ];

        let languages = vec![
            "en-US,en;q=0.9".to_string(),
            "en-GB,en;q=0.9".to_string(),
            "en-CA,en;q=0.9".to_string(),
            "en-AU,en;q=0.9".to_string(),
        ];

        let timezones = vec![
            "America/New_York".to_string(),
            "America/Los_Angeles".to_string(),
            "America/Chicago".to_string(),
            "America/Denver".to_string(),
            "Europe/London".to_string(),
            "Europe/Berlin".to_string(),
            "Australia/Sydney".to_string(),
        ];

        Self {
            viewports,
            languages,
            timezones,
        }
    }

    pub fn random_viewport(&self) -> &ViewportSize {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.viewports.len());
        &self.viewports[index]
    }

    pub fn random_language(&self) -> &str {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.languages.len());
        &self.languages[index]
    }

    pub fn random_timezone(&self) -> &str {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.timezones.len());
        &self.timezones[index]
    }

    pub fn generate_fingerprint(&self) -> BrowserFingerprint {
        BrowserFingerprint {
            viewport: self.random_viewport().clone(),
            language: self.random_language().to_string(),
            timezone: self.random_timezone().to_string(),
            platform: self.random_platform().to_string(),
            hardware_concurrency: self.random_hardware_concurrency(),
            device_memory: self.random_device_memory(),
        }
    }

    fn random_platform(&self) -> &str {
        let platforms = ["Win32", "MacIntel", "Linux x86_64"];
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..platforms.len());
        platforms[index]
    }

    fn random_hardware_concurrency(&self) -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen_range(4..=16)
    }

    fn random_device_memory(&self) -> u32 {
        let memory_options = [4, 8, 16, 32];
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..memory_options.len());
        memory_options[index]
    }
}

#[derive(Debug, Clone)]
pub struct BrowserFingerprint {
    pub viewport: ViewportSize,
    pub language: String,
    pub timezone: String,
    pub platform: String,
    pub hardware_concurrency: u32,
    pub device_memory: u32,
}

impl BrowserFingerprint {
    pub fn to_js_overrides(&self) -> HashMap<String, String> {
        let mut overrides = HashMap::new();
        
        overrides.insert(
            "navigator.language".to_string(),
            format!("'{}'", self.language.split(',').next().unwrap_or("en-US")),
        );
        
        overrides.insert(
            "navigator.languages".to_string(),
            format!("['{}']", self.language.replace(";q=0.9", "")),
        );
        
        overrides.insert(
            "navigator.platform".to_string(),
            format!("'{}'", self.platform),
        );
        
        overrides.insert(
            "navigator.hardwareConcurrency".to_string(),
            self.hardware_concurrency.to_string(),
        );
        
        overrides.insert(
            "navigator.deviceMemory".to_string(),
            self.device_memory.to_string(),
        );
        
        overrides.insert(
            "Intl.DateTimeFormat().resolvedOptions().timeZone".to_string(),
            format!("'{}'", self.timezone),
        );

        overrides
    }
}

pub fn generate_video_disable_script() -> &'static str {
    r#"
    // remove video elements and disable video playback
    (function() {
        // function to disable video elements
        function disableVideos() {
            // remove all video elements
            const videos = document.querySelectorAll('video');
            videos.forEach(video => {
                video.pause();
                video.src = '';
                video.load();
                video.style.display = 'none';
                video.remove();
            });

            // remove video containers
            const videoContainers = document.querySelectorAll(
                '[data-a-target="video-player"], .video-player, .player-video, .video-ref'
            );
            videoContainers.forEach(container => {
                container.style.display = 'none';
                container.remove();
            });

            // disable webrtc and media streams
            if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
                navigator.mediaDevices.getUserMedia = function() {
                    return Promise.reject(new Error('Media access disabled'));
                };
            }

            // override video creation
            const originalCreateElement = document.createElement;
            document.createElement = function(tagName) {
                if (tagName.toLowerCase() === 'video') {
                    const div = originalCreateElement.call(this, 'div');
                    div.style.display = 'none';
                    return div;
                }
                return originalCreateElement.call(this, tagName);
            };
        }

        // Run immediately
        disableVideos();

        // Run on DOM changes
        const observer = new MutationObserver(function(mutations) {
            mutations.forEach(function(mutation) {
                if (mutation.addedNodes.length > 0) {
                    disableVideos();
                }
            });
        });

        observer.observe(document.body, {
            childList: true,
            subtree: true
        });

        // Run periodically as backup
        setInterval(disableVideos, 5000);
    })();
    "#
}

pub fn generate_stealth_script(fingerprint: &BrowserFingerprint) -> String {
    let overrides = fingerprint.to_js_overrides();
    let mut script = String::from(r#"
    // stealth script to avoid detection
    (function() {
        // override navigator properties
    "#);

    for (property, value) in overrides {
        script.push_str(&format!(
            "        Object.defineProperty(navigator, '{}', {{ value: {}, writable: false }});\n",
            property.replace("navigator.", ""), value
        ));
    }

    script.push_str(r#"
        // Hide webdriver property
        Object.defineProperty(navigator, 'webdriver', { value: false, writable: false });
        
        // Override plugins
        Object.defineProperty(navigator, 'plugins', {
            value: [
                { name: 'Chrome PDF Plugin', description: 'Portable Document Format' },
                { name: 'Chrome PDF Viewer', description: 'PDF Viewer' },
                { name: 'Native Client', description: 'Native Client' }
            ],
            writable: false
        });

        // Override permissions
        const originalQuery = navigator.permissions.query;
        navigator.permissions.query = function(parameters) {
            return parameters.name === 'notifications' 
                ? Promise.resolve({ state: Notification.permission })
                : originalQuery.call(this, parameters);
        };

        // Hide automation indicators
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;
        
        // Override chrome runtime
        if (window.chrome && window.chrome.runtime) {
            Object.defineProperty(window.chrome.runtime, 'onConnect', { value: undefined });
            Object.defineProperty(window.chrome.runtime, 'onMessage', { value: undefined });
        }
    })();
    "#);

    script
}