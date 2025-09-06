use crate::error::ScrapingError;
use crate::parser::{ChatMessage, QualityAlert, QualityMetricsTracker};
use crate::parser::html_parser::TwitchChatParser;
use std::collections::HashSet;
use tracing::{debug, warn, info};

/// processor for checking, filtering, and removing duplicate chat messages
pub struct DataProcessor {
    parser: TwitchChatParser,
    seen_hashes: HashSet<String>,
    min_message_length: usize,
    max_message_length: usize,
    filter_spam: bool,
    filter_bots: bool,
    quality_tracker: QualityMetricsTracker,
}

impl DataProcessor {
    /// Create a new data processor with default settings
    pub fn new() -> Result<Self, ScrapingError> {
        Ok(Self {
            parser: TwitchChatParser::new()?,
            seen_hashes: HashSet::new(),
            min_message_length: 1,
            max_message_length: 500,
            filter_spam: true,
            filter_bots: true,
            quality_tracker: QualityMetricsTracker::new(),
        })
    }

    // make a processor with custom settings
    pub fn with_settings(
        min_length: usize,
        max_length: usize,
        filter_spam: bool,
        filter_bots: bool,
    ) -> Result<Self, ScrapingError> {
        Ok(Self {
            parser: TwitchChatParser::new()?,
            seen_hashes: HashSet::new(),
            min_message_length: min_length,
            max_message_length: max_length,
            filter_spam,
            filter_bots,
            quality_tracker: QualityMetricsTracker::new(),
        })
    }

    // pull chat messages from html
    pub fn parse_chat_html(&self, html: &str, streamer: &str) -> Result<Vec<ChatMessage>, ScrapingError> {
        self.parser.parse_chat_html(html, streamer)
    }

    // check if one message passes our rules
    pub fn validate_message(&self, message: &ChatMessage) -> bool {
        // Basic validation
        if !message.is_valid() {
            debug!("Message failed basic validation: {:?}", message);
            return false;
        }

        // Length validation
        let msg_len = message.message_length();
        if msg_len < self.min_message_length || msg_len > self.max_message_length {
            debug!("Message length {} outside allowed range [{}, {}]", 
                   msg_len, self.min_message_length, self.max_message_length);
            return false;
        }

        // Spam filtering
        if self.filter_spam && message.is_likely_spam() {
            debug!("Message flagged as spam: {}", message.message.text);
            return false;
        }

        // Bot filtering (simple heuristics)
        if self.filter_bots && self.is_likely_bot(&message.user.username) {
            debug!("Message from likely bot: {}", message.user.username);
            return false;
        }

        true
    }

    // remove duplicates based on content hash
    pub fn deduplicate(&mut self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let mut unique_messages = Vec::new();
        let mut new_hashes = 0;

        for message in messages {
            let hash = message.content_hash();
            if !self.seen_hashes.contains(&hash) {
                self.seen_hashes.insert(hash);
                unique_messages.push(message);
                new_hashes += 1;
            }
        }

        debug!("Deduplicated {} messages, {} new unique messages", 
               unique_messages.len(), new_hashes);
        unique_messages
    }

    // run all filters on a batch of messages with quality tracking
    pub fn apply_filters(&mut self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let initial_count = messages.len();
        let mut spam_count = 0;
        let mut bot_count = 0;
        let mut length_filtered = 0;
        let mut parse_errors = 0;
        let mut unique_users = Vec::new();
        let mut message_lengths = Vec::new();
        
        // Track streamer for metrics
        let streamer = if !messages.is_empty() {
            messages[0].streamer.clone()
        } else {
            return Vec::new();
        };

        // Categorize filtered messages for quality metrics
        let mut valid_messages = Vec::new();
        for message in messages {
            // Track unique users and message lengths for all messages
            if !unique_users.contains(&message.user.username) {
                unique_users.push(message.user.username.clone());
            }
            message_lengths.push(message.message_length());

            // Basic validation
            if !message.is_valid() {
                parse_errors += 1;
                continue;
            }

            // Length validation
            let msg_len = message.message_length();
            if msg_len < self.min_message_length || msg_len > self.max_message_length {
                length_filtered += 1;
                continue;
            }

            // Spam filtering
            if self.filter_spam && message.is_likely_spam() {
                spam_count += 1;
                continue;
            }

            // Bot filtering
            if self.filter_bots && self.is_likely_bot(&message.user.username) {
                bot_count += 1;
                continue;
            }

            valid_messages.push(message);
        }

        // Count duplicates before deduplication
        let pre_dedup_count = valid_messages.len();
        let final_messages = self.deduplicate(valid_messages);
        let duplicates_filtered = pre_dedup_count - final_messages.len();

        // Record quality metrics
        self.quality_tracker.record_batch_processed(
            &streamer,
            initial_count as u64,
            final_messages.len() as u64,
            spam_count,
            bot_count,
            length_filtered,
            duplicates_filtered as u64,
            parse_errors,
            unique_users,
            message_lengths,
        );

        debug!("Filtered {} messages down to {} after validation and deduplication", 
               initial_count, final_messages.len());
        
        final_messages
    }

    // process html and give back filtered, unique messages
    pub fn process_html(&mut self, html: &str, streamer: &str) -> Result<Vec<ChatMessage>, ScrapingError> {
        let messages = self.parse_chat_html(html, streamer)?;
        Ok(self.apply_filters(messages))
    }

    // clear the cache for duplicates
    pub fn clear_cache(&mut self) {
        self.seen_hashes.clear();
        debug!("Cleared deduplication cache");
    }

    // how many unique messages we've seen
    pub fn unique_message_count(&self) -> usize {
        self.seen_hashes.len()
    }

    // get current quality stats
    pub fn get_quality_metrics(&self) -> &crate::parser::QualityMetrics {
        self.quality_tracker.get_metrics()
    }

    // get quality stats for a specific streamer
    pub fn get_streamer_metrics(&self, streamer: &str) -> Option<&crate::parser::StreamerMetrics> {
        self.quality_tracker.get_streamer_metrics(streamer)
    }

    // look for quality issues
    pub fn check_quality_alerts(&self) -> Vec<QualityAlert> {
        self.quality_tracker.check_alerts()
    }

    // make a quality report
    pub fn generate_quality_report(&self) -> String {
        self.quality_tracker.generate_report()
    }

    // start fresh with quality metrics
    pub fn reset_quality_metrics(&mut self) {
        self.quality_tracker.reset();
    }

    // print quality alerts to the console
    pub fn log_quality_alerts(&self) {
        let alerts = self.check_quality_alerts();
        for alert in alerts {
            match alert {
                QualityAlert::Info(msg) => info!("Quality Info: {}", msg),
                QualityAlert::Warning(msg) => warn!("Quality Warning: {}", msg),
                QualityAlert::Critical(msg) => {
                    warn!("Quality Critical: {}", msg);
                    // Could also trigger additional actions like notifications
                }
            }
        }
    }

    // simple way to spot bots
    fn is_likely_bot(&self, username: &str) -> bool {
        let username_lower = username.to_lowercase();
        
        // Common bot patterns
        let bot_patterns = [
            "bot", "nightbot", "streamlabs", "moobot", "fossabot",
            "streamelements", "wizebot", "deepbot", "ankhbot", "phantombot"
        ];

        for pattern in &bot_patterns {
            if username_lower.contains(pattern) {
                return true;
            }
        }

        // Check for patterns like "user123456" (likely generated usernames)
        if username_lower.starts_with("user") && 
           username_lower.chars().skip(4).all(|c| c.is_ascii_digit()) &&
           username.len() > 8 {
            return true;
        }

        false
    }
}

impl Default for DataProcessor {
    fn default() -> Self {
        Self::new().expect("Failed to create default DataProcessor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::parser::{ChatUser, MessageContent, MessageFragment, StreamContext};

    fn create_test_message(username: &str, text: &str) -> ChatMessage {
        ChatMessage::new(
            "teststreamer".to_string(),
            Utc::now(),
            ChatUser {
                username: username.to_string(),
                display_name: username.to_string(),
                color: Some("#FF0000".to_string()),
                badges: vec![],
            },
            MessageContent {
                text: text.to_string(),
                emotes: vec![],
                fragments: vec![MessageFragment {
                    fragment_type: "text".to_string(),
                    content: text.to_string(),
                }],
            },
            StreamContext::default(),
        )
    }

    #[test]
    fn test_processor_creation() {
        let processor = DataProcessor::new();
        assert!(processor.is_ok());
    }

    #[test]
    fn test_message_validation() {
        let processor = DataProcessor::new().unwrap();
        
        let valid_message = create_test_message("user", "Hello world!");
        assert!(processor.validate_message(&valid_message));

        let short_message = create_test_message("user", "");
        assert!(!processor.validate_message(&short_message));
    }

    #[test]
    fn test_spam_filtering() {
        let processor = DataProcessor::new().unwrap();
        
        let normal_message = create_test_message("user", "Hello world!");
        assert!(processor.validate_message(&normal_message));

        let spam_message = create_test_message("user", "AAAAAAAAAAAAAAAA");
        assert!(!processor.validate_message(&spam_message));
    }

    #[test]
    fn test_bot_detection() {
        let processor = DataProcessor::new().unwrap();
        
        let human_message = create_test_message("regularuser", "Hello!");
        assert!(processor.validate_message(&human_message));

        let bot_message = create_test_message("nightbot", "Hello!");
        assert!(!processor.validate_message(&bot_message));

        let generated_user = create_test_message("user123456789", "Hello!");
        assert!(!processor.validate_message(&generated_user));
    }

    #[test]
    fn test_deduplication() {
        let mut processor = DataProcessor::new().unwrap();
        
        let message1 = create_test_message("user", "Hello world!");
        let message2 = create_test_message("user", "Hello world!"); // Duplicate
        let message3 = create_test_message("user", "Different message");

        let messages = vec![message1, message2, message3];
        let unique_messages = processor.deduplicate(messages);

        assert_eq!(unique_messages.len(), 2);
        assert_eq!(processor.unique_message_count(), 2);
    }

    #[test]
    fn test_length_filtering() {
        let processor = DataProcessor::with_settings(5, 20, false, false).unwrap();
        
        let too_short = create_test_message("user", "Hi");
        assert!(!processor.validate_message(&too_short));

        let just_right = create_test_message("user", "Hello world!");
        assert!(processor.validate_message(&just_right));

        let too_long = create_test_message("user", "This message is way too long for our configured limits");
        assert!(!processor.validate_message(&too_long));
    }

    #[test]
    fn test_quality_metrics_integration() {
        let mut processor = DataProcessor::new().unwrap();
        
        let messages = vec![
            create_test_message("user1", "Hello world!"),
            create_test_message("user1", "Hello world!"), // Duplicate (same user, same message)
            create_test_message("nightbot", "Bot message"), // Bot
            create_test_message("user3", "AAAAAAAAAAAAAAAA"), // Spam (repetitive characters)
            create_test_message("user4", "Valid message"),
        ];

        let filtered = processor.apply_filters(messages);
        
        // Should have 2 valid messages (user1 and user4)
        assert_eq!(filtered.len(), 2);
        
        // Check quality metrics
        let metrics = processor.get_quality_metrics();
        assert_eq!(metrics.total_processed, 5);
        assert_eq!(metrics.valid_messages, 2);
        assert!(metrics.spam_filtered > 0);
        assert!(metrics.bot_filtered > 0);
        assert!(metrics.duplicates_filtered > 0);
        
        // Check streamer metrics
        let streamer_metrics = processor.get_streamer_metrics("teststreamer").unwrap();
        assert_eq!(streamer_metrics.total_messages, 5);
        assert_eq!(streamer_metrics.valid_messages, 2);
    }

    #[test]
    fn test_quality_alerts() {
        let mut processor = DataProcessor::new().unwrap();
        
        // Create a scenario with high spam rate to trigger alerts
        let spam_messages: Vec<ChatMessage> = (0..100)
            .map(|i| create_test_message(&format!("user{}", i), "SPAM SPAM SPAM"))
            .collect();
        
        processor.apply_filters(spam_messages);
        
        let alerts = processor.check_quality_alerts();
        assert!(!alerts.is_empty());
        
        // Should have quality-related alerts
        let alert_messages: Vec<String> = alerts.iter().map(|alert| {
            match alert {
                QualityAlert::Info(msg) | QualityAlert::Warning(msg) | QualityAlert::Critical(msg) => msg.clone(),
            }
        }).collect();
        
        // Should have alerts about quality issues
        assert!(alert_messages.iter().any(|msg| msg.contains("quality") || msg.contains("spam")));
    }

    #[test]
    fn test_quality_report_generation() {
        let mut processor = DataProcessor::new().unwrap();
        
        let messages = vec![
            create_test_message("user1", "Hello world!"),
            create_test_message("user2", "Valid message"),
        ];

        processor.apply_filters(messages);
        
        let report = processor.generate_quality_report();
        assert!(report.contains("Quality Metrics Report"));
        assert!(report.contains("Total Processed: 2"));
        assert!(report.contains("teststreamer"));
    }

    #[test]
    fn test_metrics_reset() {
        let mut processor = DataProcessor::new().unwrap();
        
        let messages = vec![create_test_message("user1", "Hello world!")];
        processor.apply_filters(messages);
        
        assert_eq!(processor.get_quality_metrics().total_processed, 1);
        
        processor.reset_quality_metrics();
        assert_eq!(processor.get_quality_metrics().total_processed, 0);
    }
}