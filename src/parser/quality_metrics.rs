use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// Metrics for tracking how well data processing is going
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityMetrics {
    /// Total messages processed
    pub total_processed: u64,
    /// Messages that passed validation
    pub valid_messages: u64,
    /// Messages filtered out as spam
    pub spam_filtered: u64,
    /// Messages filtered out as bot messages
    pub bot_filtered: u64,
    /// Messages filtered out due to length constraints
    pub length_filtered: u64,
    /// Duplicate messages filtered out
    pub duplicates_filtered: u64,
    /// Messages with parsing errors
    pub parse_errors: u64,
    /// Quality score (0.0 to 1.0)
    pub quality_score: f64,
    /// Processing rate (messages per second)
    pub processing_rate: f64,
    /// Metrics by streamer
    pub streamer_metrics: HashMap<String, StreamerMetrics>,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
    /// Processing session start time
    pub session_start: DateTime<Utc>,
}

/// Quality stats for a specific streamer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamerMetrics {
    pub streamer_name: String,
    pub total_messages: u64,
    pub valid_messages: u64,
    pub spam_rate: f64,
    pub bot_rate: f64,
    pub duplicate_rate: f64,
    pub average_message_length: f64,
    pub unique_users: u64,
    pub last_message_time: Option<DateTime<Utc>>,
}

/// Levels for quality alerts
#[derive(Debug, Clone, PartialEq)]
pub enum QualityAlert {
    Info(String),
    Warning(String),
    Critical(String),
}

/// Tracker and reporter for quality metrics
pub struct QualityMetricsTracker {
    metrics: QualityMetrics,
    alert_thresholds: QualityThresholds,
    user_tracking: HashMap<String, HashMap<String, u64>>, // streamer -> username -> count
}

/// Settings for quality alert thresholds
#[derive(Debug, Clone)]
pub struct QualityThresholds {
    pub min_quality_score: f64,
    pub max_spam_rate: f64,
    pub max_bot_rate: f64,
    pub max_duplicate_rate: f64,
    pub min_processing_rate: f64,
}

impl Default for QualityThresholds {
    fn default() -> Self {
        Self {
            min_quality_score: 0.7,
            max_spam_rate: 0.3,
            max_bot_rate: 0.2,
            max_duplicate_rate: 0.4,
            min_processing_rate: 10.0,
        }
    }
}

impl QualityMetricsTracker {
    // make a new quality metrics tracker
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            metrics: QualityMetrics {
                total_processed: 0,
                valid_messages: 0,
                spam_filtered: 0,
                bot_filtered: 0,
                length_filtered: 0,
                duplicates_filtered: 0,
                parse_errors: 0,
                quality_score: 1.0,
                processing_rate: 0.0,
                streamer_metrics: HashMap::new(),
                last_updated: now,
                session_start: now,
            },
            alert_thresholds: QualityThresholds::default(),
            user_tracking: HashMap::new(),
        }
    }

    // make a tracker with custom thresholds
    pub fn with_thresholds(thresholds: QualityThresholds) -> Self {
        let mut tracker = Self::new();
        tracker.alert_thresholds = thresholds;
        tracker
    }

    // record a batch of messages processed
    pub fn record_batch_processed(
        &mut self,
        streamer: &str,
        total_messages: u64,
        valid_messages: u64,
        spam_count: u64,
        bot_count: u64,
        length_filtered: u64,
        duplicates: u64,
        parse_errors: u64,
        unique_users: Vec<String>,
        message_lengths: Vec<usize>,
    ) {
        // Update global metrics
        self.metrics.total_processed += total_messages;
        self.metrics.valid_messages += valid_messages;
        self.metrics.spam_filtered += spam_count;
        self.metrics.bot_filtered += bot_count;
        self.metrics.length_filtered += length_filtered;
        self.metrics.duplicates_filtered += duplicates;
        self.metrics.parse_errors += parse_errors;
        self.metrics.last_updated = Utc::now();

        // Calculate processing rate
        let session_duration = (self.metrics.last_updated - self.metrics.session_start)
            .num_seconds() as f64;
        if session_duration > 0.0 {
            self.metrics.processing_rate = self.metrics.total_processed as f64 / session_duration;
        }

        // Update streamer-specific metrics
        let streamer_metrics = self.metrics.streamer_metrics
            .entry(streamer.to_string())
            .or_insert_with(|| StreamerMetrics {
                streamer_name: streamer.to_string(),
                total_messages: 0,
                valid_messages: 0,
                spam_rate: 0.0,
                bot_rate: 0.0,
                duplicate_rate: 0.0,
                average_message_length: 0.0,
                unique_users: 0,
                last_message_time: None,
            });

        streamer_metrics.total_messages += total_messages;
        streamer_metrics.valid_messages += valid_messages;
        streamer_metrics.last_message_time = Some(self.metrics.last_updated);

        // Calculate rates for this streamer based on total messages for this streamer
        if streamer_metrics.total_messages > 0 {
            // Calculate cumulative rates for this streamer
            let total_spam = (streamer_metrics.spam_rate * (streamer_metrics.total_messages - total_messages) as f64) + spam_count as f64;
            let total_bot = (streamer_metrics.bot_rate * (streamer_metrics.total_messages - total_messages) as f64) + bot_count as f64;
            let total_duplicates = (streamer_metrics.duplicate_rate * (streamer_metrics.total_messages - total_messages) as f64) + duplicates as f64;
            
            streamer_metrics.spam_rate = total_spam / streamer_metrics.total_messages as f64;
            streamer_metrics.bot_rate = total_bot / streamer_metrics.total_messages as f64;
            streamer_metrics.duplicate_rate = total_duplicates / streamer_metrics.total_messages as f64;
        }

        // Calculate average message length
        if !message_lengths.is_empty() {
            let total_length: usize = message_lengths.iter().sum();
            streamer_metrics.average_message_length = total_length as f64 / message_lengths.len() as f64;
        }

        // Track unique users
        let user_set = self.user_tracking
            .entry(streamer.to_string())
            .or_insert_with(HashMap::new);
        
        for user in unique_users {
            *user_set.entry(user).or_insert(0) += 1;
        }
        streamer_metrics.unique_users = user_set.len() as u64;

        // Update global quality score
        self.update_quality_score();

        debug!("Updated metrics for streamer {}: {} total, {} valid", 
               streamer, total_messages, valid_messages);
    }

    // update the overall quality score
    fn update_quality_score(&mut self) {
        if self.metrics.total_processed == 0 {
            self.metrics.quality_score = 1.0;
            return;
        }

        let valid_rate = self.metrics.valid_messages as f64 / self.metrics.total_processed as f64;
        let spam_rate = self.metrics.spam_filtered as f64 / self.metrics.total_processed as f64;
        let bot_rate = self.metrics.bot_filtered as f64 / self.metrics.total_processed as f64;
        let error_rate = self.metrics.parse_errors as f64 / self.metrics.total_processed as f64;

        // Quality score calculation (weighted average)
        let quality_factors = [
            (valid_rate, 0.4),           // 40% weight on valid messages
            (1.0 - spam_rate, 0.25),     // 25% weight on low spam rate
            (1.0 - bot_rate, 0.2),       // 20% weight on low bot rate
            (1.0 - error_rate, 0.15),    // 15% weight on low error rate
        ];

        self.metrics.quality_score = quality_factors
            .iter()
            .map(|(score, weight)| score * weight)
            .sum();

        // Ensure score is between 0.0 and 1.0
        self.metrics.quality_score = self.metrics.quality_score.max(0.0).min(1.0);
    }

    // check for quality alerts based on metrics
    pub fn check_alerts(&self) -> Vec<QualityAlert> {
        let mut alerts = Vec::new();

        // Global quality score alert
        if self.metrics.quality_score < self.alert_thresholds.min_quality_score {
            alerts.push(QualityAlert::Warning(format!(
                "Overall quality score ({:.2}) below threshold ({:.2})",
                self.metrics.quality_score, self.alert_thresholds.min_quality_score
            )));
        }

        // Processing rate alert
        if self.metrics.processing_rate < self.alert_thresholds.min_processing_rate {
            alerts.push(QualityAlert::Warning(format!(
                "Processing rate ({:.1} msg/s) below threshold ({:.1} msg/s)",
                self.metrics.processing_rate, self.alert_thresholds.min_processing_rate
            )));
        }

        // Check streamer-specific alerts
        for (streamer, metrics) in &self.metrics.streamer_metrics {
            if metrics.spam_rate > self.alert_thresholds.max_spam_rate {
                alerts.push(QualityAlert::Warning(format!(
                    "High spam rate for {}: {:.1}% (threshold: {:.1}%)",
                    streamer, metrics.spam_rate * 100.0, self.alert_thresholds.max_spam_rate * 100.0
                )));
            }

            if metrics.bot_rate > self.alert_thresholds.max_bot_rate {
                alerts.push(QualityAlert::Warning(format!(
                    "High bot rate for {}: {:.1}% (threshold: {:.1}%)",
                    streamer, metrics.bot_rate * 100.0, self.alert_thresholds.max_bot_rate * 100.0
                )));
            }

            if metrics.duplicate_rate > self.alert_thresholds.max_duplicate_rate {
                alerts.push(QualityAlert::Info(format!(
                    "High duplicate rate for {}: {:.1}% (threshold: {:.1}%)",
                    streamer, metrics.duplicate_rate * 100.0, self.alert_thresholds.max_duplicate_rate * 100.0
                )));
            }

            // Check for inactive streamers
            if let Some(last_msg_time) = metrics.last_message_time {
                let inactive_duration = Utc::now() - last_msg_time;
                if inactive_duration.num_minutes() > 30 {
                    alerts.push(QualityAlert::Info(format!(
                        "No messages from {} for {} minutes",
                        streamer, inactive_duration.num_minutes()
                    )));
                }
            }
        }

        // Critical alerts
        let error_rate = if self.metrics.total_processed > 0 {
            self.metrics.parse_errors as f64 / self.metrics.total_processed as f64
        } else {
            0.0
        };

        if error_rate > 0.1 {
            alerts.push(QualityAlert::Critical(format!(
                "High error rate: {:.1}% of messages failed to parse",
                error_rate * 100.0
            )));
        }

        alerts
    }

    // get current metrics snapshot
    pub fn get_metrics(&self) -> &QualityMetrics {
        &self.metrics
    }

    // get metrics for a specific streamer
    pub fn get_streamer_metrics(&self, streamer: &str) -> Option<&StreamerMetrics> {
        self.metrics.streamer_metrics.get(streamer)
    }

    // reset all metrics for a new session
    pub fn reset(&mut self) {
        let now = Utc::now();
        self.metrics = QualityMetrics {
            total_processed: 0,
            valid_messages: 0,
            spam_filtered: 0,
            bot_filtered: 0,
            length_filtered: 0,
            duplicates_filtered: 0,
            parse_errors: 0,
            quality_score: 1.0,
            processing_rate: 0.0,
            streamer_metrics: HashMap::new(),
            last_updated: now,
            session_start: now,
        };
        self.user_tracking.clear();
        info!("Quality metrics reset for new session");
    }

    // make a quality report
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str("=== Quality Metrics Report ===\n");
        report.push_str(&format!("Session Duration: {:.1} minutes\n", 
            (self.metrics.last_updated - self.metrics.session_start).num_minutes()));
        report.push_str(&format!("Total Processed: {}\n", self.metrics.total_processed));
        report.push_str(&format!("Valid Messages: {} ({:.1}%)\n", 
            self.metrics.valid_messages,
            if self.metrics.total_processed > 0 {
                self.metrics.valid_messages as f64 / self.metrics.total_processed as f64 * 100.0
            } else { 0.0 }
        ));
        report.push_str(&format!("Quality Score: {:.2}\n", self.metrics.quality_score));
        report.push_str(&format!("Processing Rate: {:.1} msg/s\n", self.metrics.processing_rate));
        report.push_str(&format!("Spam Filtered: {} ({:.1}%)\n", 
            self.metrics.spam_filtered,
            if self.metrics.total_processed > 0 {
                self.metrics.spam_filtered as f64 / self.metrics.total_processed as f64 * 100.0
            } else { 0.0 }
        ));
        report.push_str(&format!("Bot Filtered: {} ({:.1}%)\n", 
            self.metrics.bot_filtered,
            if self.metrics.total_processed > 0 {
                self.metrics.bot_filtered as f64 / self.metrics.total_processed as f64 * 100.0
            } else { 0.0 }
        ));
        report.push_str(&format!("Duplicates Filtered: {}\n", self.metrics.duplicates_filtered));
        report.push_str(&format!("Parse Errors: {}\n", self.metrics.parse_errors));

        report.push_str("\n=== Streamer Breakdown ===\n");
        for (streamer, metrics) in &self.metrics.streamer_metrics {
            report.push_str(&format!("\n{}: {} messages, {} unique users, avg length: {:.1}\n",
                streamer, metrics.total_messages, metrics.unique_users, metrics.average_message_length));
            report.push_str(&format!("  Spam: {:.1}%, Bot: {:.1}%, Duplicates: {:.1}%\n",
                metrics.spam_rate * 100.0, metrics.bot_rate * 100.0, metrics.duplicate_rate * 100.0));
        }

        let alerts = self.check_alerts();
        if !alerts.is_empty() {
            report.push_str("\n=== Active Alerts ===\n");
            for alert in alerts {
                match alert {
                    QualityAlert::Info(msg) => report.push_str(&format!("INFO: {}\n", msg)),
                    QualityAlert::Warning(msg) => report.push_str(&format!("WARNING: {}\n", msg)),
                    QualityAlert::Critical(msg) => report.push_str(&format!("CRITICAL: {}\n", msg)),
                }
            }
        }

        report
    }
}

impl Default for QualityMetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_tracker_creation() {
        let tracker = QualityMetricsTracker::new();
        assert_eq!(tracker.metrics.total_processed, 0);
        assert_eq!(tracker.metrics.quality_score, 1.0);
    }

    #[test]
    fn test_batch_recording() {
        let mut tracker = QualityMetricsTracker::new();
        
        tracker.record_batch_processed(
            "teststreamer",
            100, // total
            80,  // valid
            10,  // spam
            5,   // bot
            3,   // length filtered
            2,   // duplicates
            0,   // parse errors
            vec!["user1".to_string(), "user2".to_string()],
            vec![10, 15, 20],
        );

        assert_eq!(tracker.metrics.total_processed, 100);
        assert_eq!(tracker.metrics.valid_messages, 80);
        assert_eq!(tracker.metrics.spam_filtered, 10);
        
        let streamer_metrics = tracker.get_streamer_metrics("teststreamer").unwrap();
        assert_eq!(streamer_metrics.total_messages, 100);
        assert_eq!(streamer_metrics.unique_users, 2);
        assert_eq!(streamer_metrics.average_message_length, 15.0);
    }

    #[test]
    fn test_quality_score_calculation() {
        let mut tracker = QualityMetricsTracker::new();
        
        // Perfect quality scenario
        tracker.record_batch_processed(
            "teststreamer",
            100, 100, 0, 0, 0, 0, 0,
            vec!["user1".to_string()],
            vec![10],
        );
        
        assert!(tracker.metrics.quality_score > 0.9);
        
        // Poor quality scenario - reset tracker first
        let mut poor_tracker = QualityMetricsTracker::new();
        poor_tracker.record_batch_processed(
            "teststreamer2",
            100, 20, 40, 30, 10, 0, 0,
            vec!["user1".to_string()],
            vec![10],
        );
        
        assert!(poor_tracker.metrics.quality_score < 0.7);
    }

    #[test]
    fn test_alert_generation() {
        let mut tracker = QualityMetricsTracker::new();
        
        // Create scenario that should trigger alerts
        // Default thresholds: spam_rate: 0.3 (30%), bot_rate: 0.2 (20%)
        // So we need spam_rate > 30% and bot_rate > 20%
        tracker.record_batch_processed(
            "teststreamer",
            100, 20, 40, 30, 0, 0, 0, // 40% spam, 30% bot - should trigger both alerts
            vec!["user1".to_string()],
            vec![10],
        );
        
        let alerts = tracker.check_alerts();
        assert!(!alerts.is_empty());
        
        // Check that we have spam and bot rate alerts
        let alert_messages: Vec<String> = alerts.iter().map(|alert| {
            match alert {
                QualityAlert::Info(msg) | QualityAlert::Warning(msg) | QualityAlert::Critical(msg) => msg.clone(),
            }
        }).collect();
        

        assert!(alert_messages.iter().any(|msg| msg.contains("spam rate")));
        assert!(alert_messages.iter().any(|msg| msg.contains("bot rate")));
    }

    #[test]
    fn test_report_generation() {
        let mut tracker = QualityMetricsTracker::new();
        
        tracker.record_batch_processed(
            "teststreamer",
            100, 80, 10, 5, 3, 2, 0,
            vec!["user1".to_string(), "user2".to_string()],
            vec![10, 15, 20],
        );
        
        let report = tracker.generate_report();
        assert!(report.contains("Quality Metrics Report"));
        assert!(report.contains("Total Processed: 100"));
        assert!(report.contains("teststreamer"));
    }
}