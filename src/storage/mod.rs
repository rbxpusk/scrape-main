use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::parser::chat_message::ChatMessage;
use crate::config::FileConfigManager;
use crate::error::{Result, ScrapingError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_messages: u64,
    pub files_created: u32,
    pub disk_usage: u64,
    pub last_rotation: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub created: DateTime<Utc>,
    pub message_count: u64,
}

#[async_trait]
pub trait StorageManager {
    async fn store_messages(&self, messages: Vec<ChatMessage>) -> Result<()>;
    async fn setup_rotation(&self) -> Result<()>;
    async fn get_storage_stats(&self) -> Result<StorageStats>;
}

pub trait OutputFormatter {
    fn format_messages(&self, messages: &[ChatMessage]) -> Result<String>;
    fn file_extension(&self) -> &str;
    fn header(&self) -> Option<String>;
}

pub struct JsonFormatter;
pub struct CsvFormatter {
    columns: Vec<String>,
}

impl OutputFormatter for JsonFormatter {
    fn format_messages(&self, messages: &[ChatMessage]) -> Result<String> {
        let mut output = String::new();
        for message in messages {
            let json_line = serde_json::to_string(message)
                .map_err(|e| ScrapingError::StorageError(format!("JSON serialization failed: {}", e)))?;
            output.push_str(&json_line);
            output.push('\n');
        }
        Ok(output)
    }

    fn file_extension(&self) -> &str {
        "jsonl"
    }

    fn header(&self) -> Option<String> {
        None
    }
}

impl CsvFormatter {
    pub fn new(columns: Vec<String>) -> Self {
        Self { columns }
    }

    pub fn default_columns() -> Vec<String> {
        vec![
            "id".to_string(),
            "timestamp".to_string(),
            "streamer".to_string(),
            "username".to_string(),
            "display_name".to_string(),
            "message_text".to_string(),
            "user_color".to_string(),
            "badges".to_string(),
            "viewer_count".to_string(),
            "game_category".to_string(),
            "stream_title".to_string(),
        ]
    }

    fn escape_csv_field(field: &str) -> String {
        if field.contains(',') || field.contains('"') || field.contains('\n') {
            format!("\"{}\"", field.replace('"', "\"\""))
        } else {
            field.to_string()
        }
    }

    fn extract_field_value(&self, message: &ChatMessage, column: &str) -> String {
        match column {
            "id" => message.id.clone(),
            "timestamp" => message.timestamp.to_rfc3339(),
            "streamer" => message.streamer.clone(),
            "username" => message.user.username.clone(),
            "display_name" => message.user.display_name.clone(),
            "message_text" => message.message.text.clone(),
            "user_color" => message.user.color.as_deref().unwrap_or("").to_string(),
            "badges" => message.user.badges.join(";"),
            "viewer_count" => message.context.viewer_count.map_or(String::new(), |v| v.to_string()),
            "game_category" => message.context.game_category.as_deref().unwrap_or("").to_string(),
            "stream_title" => message.context.stream_title.as_deref().unwrap_or("").to_string(),
            _ => String::new(),
        }
    }
}

impl OutputFormatter for CsvFormatter {
    fn format_messages(&self, messages: &[ChatMessage]) -> Result<String> {
        let mut output = String::new();
        
        for message in messages {
            let mut row = Vec::new();
            for column in &self.columns {
                let value = self.extract_field_value(message, column);
                row.push(Self::escape_csv_field(&value));
            }
            output.push_str(&row.join(","));
            output.push('\n');
        }
        
        Ok(output)
    }

    fn file_extension(&self) -> &str {
        "csv"
    }

    fn header(&self) -> Option<String> {
        Some(self.columns.join(","))
    }
}

pub struct FileStorageManager {
    output_dir: PathBuf,
    formatter: Box<dyn OutputFormatter + Send + Sync>,
    rotation_size: u64,
    rotation_time: chrono::Duration,
    current_files: Arc<Mutex<HashMap<String, FileInfo>>>,
    stats: Arc<Mutex<StorageStats>>,
}

impl FileStorageManager {
    pub fn new(
        output_dir: PathBuf,
        format: String,
        rotation_size_str: String,
        rotation_time_str: String,
    ) -> Result<Self> {
        // Parse rotation size and time
        let rotation_size = FileConfigManager::parse_size_to_bytes(&rotation_size_str)?;
        let rotation_time = chrono::Duration::from_std(
            FileConfigManager::parse_time_to_duration(&rotation_time_str)?
        ).map_err(|e| ScrapingError::ConfigError(format!("Invalid rotation time: {}", e)))?;

        // Create formatter based on format type
        let formatter: Box<dyn OutputFormatter + Send + Sync> = match format.as_str() {
            "json" => Box::new(JsonFormatter),
            "csv" => Box::new(CsvFormatter::new(CsvFormatter::default_columns())),
            _ => return Err(ScrapingError::ConfigError(format!("Unsupported format: {}", format)).into()),
        };

        Ok(Self {
            output_dir,
            formatter,
            rotation_size,
            rotation_time,
            current_files: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(StorageStats {
                total_messages: 0,
                files_created: 0,
                disk_usage: 0,
                last_rotation: None,
            })),
        })
    }

    pub fn with_csv_columns(
        output_dir: PathBuf,
        columns: Vec<String>,
        rotation_size_str: String,
        rotation_time_str: String,
    ) -> Result<Self> {
        let rotation_size = FileConfigManager::parse_size_to_bytes(&rotation_size_str)?;
        let rotation_time = chrono::Duration::from_std(
            FileConfigManager::parse_time_to_duration(&rotation_time_str)?
        ).map_err(|e| ScrapingError::ConfigError(format!("Invalid rotation time: {}", e)))?;

        let formatter = Box::new(CsvFormatter::new(columns));

        Ok(Self {
            output_dir,
            formatter,
            rotation_size,
            rotation_time,
            current_files: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(StorageStats {
                total_messages: 0,
                files_created: 0,
                disk_usage: 0,
                last_rotation: None,
            })),
        })
    }

    async fn get_file_path(&self, streamer: &str, timestamp: DateTime<Utc>) -> PathBuf {
        let date_str = timestamp.format("%Y-%m-%d").to_string();
        let time_str = timestamp.format("%H-%M-%S").to_string();
        
        // Create directory structure: output_dir/streamer/YYYY-MM-DD/
        let dir_path = self.output_dir
            .join(streamer)
            .join(&date_str);
        
        // Create filename with timestamp and extension
        let filename = format!("chat_{}_{}.{}", 
            date_str, 
            time_str, 
            self.formatter.file_extension()
        );
        
        dir_path.join(filename)
    }

    async fn ensure_directory_exists(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ScrapingError::StorageError(format!("Failed to create directory: {}", e)))?;
        }
        Ok(())
    }

    async fn should_rotate_file(&self, file_info: &FileInfo) -> bool {
        // Check size-based rotation
        if file_info.size >= self.rotation_size {
            debug!("File {} needs rotation due to size: {} bytes", file_info.path.display(), file_info.size);
            return true;
        }

        // Check time-based rotation
        let now = Utc::now();
        let age = now.signed_duration_since(file_info.created);
        if age >= self.rotation_time {
            debug!("File {} needs rotation due to age: {} minutes", 
                file_info.path.display(), 
                age.num_minutes()
            );
            return true;
        }

        false
    }

    async fn write_to_file(&self, file_path: &Path, content: &str, is_new_file: bool) -> Result<u64> {
        self.ensure_directory_exists(file_path).await?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
            .map_err(|e| ScrapingError::StorageError(format!("Failed to open file: {}", e)))?;

        let mut bytes_written = 0;

        // Write header for new files if formatter provides one
        if is_new_file {
            if let Some(header) = self.formatter.header() {
                let header_line = format!("{}\n", header);
                file.write_all(header_line.as_bytes())
                    .map_err(|e| ScrapingError::StorageError(format!("Failed to write header: {}", e)))?;
                bytes_written += header_line.len() as u64;
            }
        }

        // Write content
        file.write_all(content.as_bytes())
            .map_err(|e| ScrapingError::StorageError(format!("Failed to write content: {}", e)))?;
        bytes_written += content.len() as u64;

        file.flush()
            .map_err(|e| ScrapingError::StorageError(format!("Failed to flush file: {}", e)))?;

        Ok(bytes_written)
    }

    async fn update_file_info(&self, streamer: &str, file_path: PathBuf, bytes_written: u64, message_count: u64) {
        let mut current_files = self.current_files.lock().await;
        
        match current_files.get_mut(streamer) {
            Some(file_info) => {
                file_info.size += bytes_written;
                file_info.message_count += message_count;
            }
            None => {
                current_files.insert(streamer.to_string(), FileInfo {
                    path: file_path,
                    size: bytes_written,
                    created: Utc::now(),
                    message_count,
                });
            }
        }
    }

    async fn rotate_file_if_needed(&self, streamer: &str) -> Result<()> {
        let mut current_files = self.current_files.lock().await;
        
        if let Some(file_info) = current_files.get(streamer) {
            if self.should_rotate_file(file_info).await {
                info!("Rotating file for streamer: {}", streamer);
                current_files.remove(streamer);
                
                let mut stats = self.stats.lock().await;
                stats.last_rotation = Some(Utc::now());
            }
        }
        
        Ok(())
    }

    async fn calculate_disk_usage(&self) -> u64 {
        let mut total_size = 0;
        
        if let Ok(entries) = fs::read_dir(&self.output_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_size += metadata.len();
                    } else if metadata.is_dir() {
                        total_size += self.calculate_directory_size(&entry.path());
                    }
                }
            }
        }
        
        total_size
    }

    fn calculate_directory_size(&self, dir_path: &Path) -> u64 {
        let mut total_size = 0;
        
        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_size += metadata.len();
                    } else if metadata.is_dir() {
                        total_size += self.calculate_directory_size(&entry.path());
                    }
                }
            }
        }
        
        total_size
    }
}

#[async_trait]
impl StorageManager for FileStorageManager {
    async fn store_messages(&self, messages: Vec<ChatMessage>) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        debug!("Storing {} messages", messages.len());

        // Group messages by streamer
        let mut messages_by_streamer: HashMap<String, Vec<ChatMessage>> = HashMap::new();
        for message in messages {
            messages_by_streamer
                .entry(message.streamer.clone())
                .or_insert_with(Vec::new)
                .push(message);
        }

        // Process each streamer's messages
        for (streamer, streamer_messages) in messages_by_streamer {
            // Check if we need to rotate the current file
            self.rotate_file_if_needed(&streamer).await?;

            // Get or create file path
            let timestamp = streamer_messages[0].timestamp;
            let file_path = self.get_file_path(&streamer, timestamp).await;
            
            // Check if this is a new file
            let current_files = self.current_files.lock().await;
            let is_new_file = !current_files.contains_key(&streamer) || 
                             current_files.get(&streamer).unwrap().path != file_path;
            drop(current_files);

            // Format messages
            let formatted_content = self.formatter.format_messages(&streamer_messages)?;

            // Write to file
            let bytes_written = self.write_to_file(&file_path, &formatted_content, is_new_file).await?;

            // Update file info and stats
            self.update_file_info(&streamer, file_path, bytes_written, streamer_messages.len() as u64).await;

            let mut stats = self.stats.lock().await;
            stats.total_messages += streamer_messages.len() as u64;
            if is_new_file {
                stats.files_created += 1;
            }
        }

        debug!("Successfully stored messages");
        Ok(())
    }

    async fn setup_rotation(&self) -> Result<()> {
        info!("Setting up file rotation system");
        
        // Create output directory if it doesn't exist
        fs::create_dir_all(&self.output_dir)
            .map_err(|e| ScrapingError::StorageError(format!("Failed to create output directory: {}", e)))?;

        // Scan existing files and populate current_files
        let mut current_files = self.current_files.lock().await;
        let mut stats = self.stats.lock().await;
        
        if let Ok(entries) = fs::read_dir(&self.output_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let streamer = entry.file_name().to_string_lossy().to_string();
                    
                    // Find the most recent file for this streamer
                    if let Ok(streamer_entries) = fs::read_dir(entry.path()) {
                        for date_entry in streamer_entries.flatten() {
                            if date_entry.path().is_dir() {
                                if let Ok(file_entries) = fs::read_dir(date_entry.path()) {
                                    for file_entry in file_entries.flatten() {
                                        if file_entry.path().is_file() {
                                            if let Ok(metadata) = file_entry.metadata() {
                                                let created = metadata.created()
                                                    .map(|t| DateTime::<Utc>::from(t))
                                                    .unwrap_or_else(|_| Utc::now());
                                                
                                                // Update or insert file info for most recent file
                                                match current_files.get(&streamer) {
                                                    Some(existing) if existing.created < created => {
                                                        current_files.insert(streamer.clone(), FileInfo {
                                                            path: file_entry.path(),
                                                            size: metadata.len(),
                                                            created,
                                                            message_count: 0, // We don't track this for existing files
                                                        });
                                                    }
                                                    None => {
                                                        current_files.insert(streamer.clone(), FileInfo {
                                                            path: file_entry.path(),
                                                            size: metadata.len(),
                                                            created,
                                                            message_count: 0,
                                                        });
                                                    }
                                                    _ => {} // Keep existing newer file
                                                }
                                                
                                                stats.files_created += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        info!("File rotation system initialized with {} existing files", current_files.len());
        Ok(())
    }

    async fn get_storage_stats(&self) -> Result<StorageStats> {
        let mut stats = self.stats.lock().await;
        stats.disk_usage = self.calculate_disk_usage().await;
        Ok(stats.clone())
    }
}
#[cfg(
test)]
mod tests {
    use super::*;
    use crate::parser::chat_message::{ChatUser, MessageContent, MessageFragment, StreamContext};
    use tempfile::tempdir;

    fn create_test_message(streamer: &str, username: &str, text: &str) -> ChatMessage {
        ChatMessage::new(
            streamer.to_string(),
            Utc::now(),
            ChatUser {
                username: username.to_string(),
                display_name: username.to_string(),
                color: Some("#FF0000".to_string()),
                badges: vec!["subscriber".to_string()],
            },
            MessageContent {
                text: text.to_string(),
                emotes: vec![],
                fragments: vec![MessageFragment {
                    fragment_type: "text".to_string(),
                    content: text.to_string(),
                }],
            },
            StreamContext {
                viewer_count: Some(1000),
                game_category: Some("Just Chatting".to_string()),
                stream_title: Some("Test Stream".to_string()),
            },
        )
    }

    #[test]
    fn test_json_formatter() {
        let formatter = JsonFormatter;
        let messages = vec![
            create_test_message("teststreamer", "user1", "Hello world!"),
            create_test_message("teststreamer", "user2", "How are you?"),
        ];

        let result = formatter.format_messages(&messages).unwrap();
        
        // Should contain two JSON lines
        let lines: Vec<&str> = result.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        
        // Each line should be valid JSON
        for line in lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }

        assert_eq!(formatter.file_extension(), "jsonl");
        assert!(formatter.header().is_none());
    }

    #[test]
    fn test_csv_formatter() {
        let columns = vec!["username".to_string(), "message_text".to_string(), "streamer".to_string()];
        let formatter = CsvFormatter::new(columns.clone());
        let messages = vec![
            create_test_message("teststreamer", "user1", "Hello world!"),
            create_test_message("teststreamer", "user2", "How are you?"),
        ];

        let result = formatter.format_messages(&messages).unwrap();
        
        // Should contain two CSV lines
        let lines: Vec<&str> = result.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        
        // Check first line content
        assert!(lines[0].contains("user1"));
        assert!(lines[0].contains("Hello world!"));
        assert!(lines[0].contains("teststreamer"));

        assert_eq!(formatter.file_extension(), "csv");
        assert_eq!(formatter.header(), Some("username,message_text,streamer".to_string()));
    }

    #[test]
    fn test_csv_field_escaping() {
        let text_with_comma = "Hello, world!";
        let text_with_quotes = "He said \"Hello\"";
        let text_with_newline = "Line 1\nLine 2";

        assert_eq!(CsvFormatter::escape_csv_field(text_with_comma), "\"Hello, world!\"");
        assert_eq!(CsvFormatter::escape_csv_field(text_with_quotes), "\"He said \"\"Hello\"\"\"");
        assert_eq!(CsvFormatter::escape_csv_field(text_with_newline), "\"Line 1\nLine 2\"");
        assert_eq!(CsvFormatter::escape_csv_field("normal text"), "normal text");
    }

    #[test]
    fn test_csv_default_columns() {
        let columns = CsvFormatter::default_columns();
        let expected = vec![
            "id", "timestamp", "streamer", "username", "display_name", 
            "message_text", "user_color", "badges", "viewer_count", 
            "game_category", "stream_title"
        ];
        assert_eq!(columns, expected);
    }

    #[tokio::test]
    async fn test_file_storage_manager_creation() {
        let temp_dir = tempdir().unwrap();
        
        // Test JSON format
        let json_manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();
        
        assert_eq!(json_manager.formatter.file_extension(), "jsonl");

        // Test CSV format
        let csv_manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "csv".to_string(),
            "50MB".to_string(),
            "30m".to_string(),
        ).unwrap();
        
        assert_eq!(csv_manager.formatter.file_extension(), "csv");

        // Test invalid format
        let invalid_result = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "invalid".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        );
        
        assert!(invalid_result.is_err());
    }

    #[tokio::test]
    async fn test_csv_with_custom_columns() {
        let temp_dir = tempdir().unwrap();
        let custom_columns = vec!["username".to_string(), "message_text".to_string()];
        
        let manager = FileStorageManager::with_csv_columns(
            temp_dir.path().to_path_buf(),
            custom_columns.clone(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();
        
        assert_eq!(manager.formatter.file_extension(), "csv");
    }

    #[tokio::test]
    async fn test_file_path_generation() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();

        let timestamp = DateTime::parse_from_rfc3339("2024-01-15T10:30:45Z").unwrap().with_timezone(&Utc);
        let file_path = manager.get_file_path("teststreamer", timestamp).await;

        let expected_path = temp_dir.path()
            .join("teststreamer")
            .join("2024-01-15")
            .join("chat_2024-01-15_10-30-45.jsonl");

        assert_eq!(file_path, expected_path);
    }

    #[tokio::test]
    async fn test_setup_rotation() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();

        // Setup rotation should create the output directory
        manager.setup_rotation().await.unwrap();
        assert!(temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_store_messages_json() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();

        manager.setup_rotation().await.unwrap();

        let messages = vec![
            create_test_message("teststreamer", "user1", "Hello world!"),
            create_test_message("teststreamer", "user2", "How are you?"),
        ];

        manager.store_messages(messages).await.unwrap();

        // Check that files were created
        let streamer_dir = temp_dir.path().join("teststreamer");
        assert!(streamer_dir.exists());

        // Find the created file
        let mut found_file = false;
        for entry in std::fs::read_dir(&streamer_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                for date_entry in std::fs::read_dir(entry.path()).unwrap() {
                    let date_entry = date_entry.unwrap();
                    if date_entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        found_file = true;
                        
                        // Check file content
                        let content = std::fs::read_to_string(date_entry.path()).unwrap();
                        let lines: Vec<&str> = content.trim().split('\n').collect();
                        assert_eq!(lines.len(), 2);
                        
                        // Verify JSON content
                        for line in lines {
                            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
                            assert!(parsed["user"]["username"].is_string());
                            assert!(parsed["message"]["text"].is_string());
                        }
                    }
                }
            }
        }
        assert!(found_file, "No JSON file was created");
    }

    #[tokio::test]
    async fn test_store_messages_csv() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "csv".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();

        manager.setup_rotation().await.unwrap();

        let messages = vec![
            create_test_message("teststreamer", "user1", "Hello world!"),
            create_test_message("teststreamer", "user2", "How are you?"),
        ];

        manager.store_messages(messages).await.unwrap();

        // Find the created CSV file
        let streamer_dir = temp_dir.path().join("teststreamer");
        let mut found_file = false;
        
        for entry in std::fs::read_dir(&streamer_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                for date_entry in std::fs::read_dir(entry.path()).unwrap() {
                    let date_entry = date_entry.unwrap();
                    if date_entry.path().extension().and_then(|s| s.to_str()) == Some("csv") {
                        found_file = true;
                        
                        // Check file content
                        let content = std::fs::read_to_string(date_entry.path()).unwrap();
                        let lines: Vec<&str> = content.trim().split('\n').collect();
                        
                        // Should have header + 2 data lines
                        assert_eq!(lines.len(), 3);
                        
                        // Check header
                        assert!(lines[0].contains("username"));
                        assert!(lines[0].contains("message_text"));
                        
                        // Check data lines
                        assert!(lines[1].contains("user1"));
                        assert!(lines[1].contains("Hello world!"));
                        assert!(lines[2].contains("user2"));
                        assert!(lines[2].contains("How are you?"));
                    }
                }
            }
        }
        assert!(found_file, "No CSV file was created");
    }

    #[tokio::test]
    async fn test_multiple_streamers() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();

        manager.setup_rotation().await.unwrap();

        let messages = vec![
            create_test_message("streamer1", "user1", "Hello from streamer1!"),
            create_test_message("streamer2", "user2", "Hello from streamer2!"),
            create_test_message("streamer1", "user3", "Another message for streamer1!"),
        ];

        manager.store_messages(messages).await.unwrap();

        // Check that both streamer directories were created
        assert!(temp_dir.path().join("streamer1").exists());
        assert!(temp_dir.path().join("streamer2").exists());
    }

    #[tokio::test]
    async fn test_storage_stats() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();

        manager.setup_rotation().await.unwrap();

        // Initial stats
        let initial_stats = manager.get_storage_stats().await.unwrap();
        assert_eq!(initial_stats.total_messages, 0);
        assert_eq!(initial_stats.files_created, 0);

        // Store some messages
        let messages = vec![
            create_test_message("teststreamer", "user1", "Hello world!"),
            create_test_message("teststreamer", "user2", "How are you?"),
        ];

        manager.store_messages(messages).await.unwrap();

        // Check updated stats
        let updated_stats = manager.get_storage_stats().await.unwrap();
        assert_eq!(updated_stats.total_messages, 2);
        assert_eq!(updated_stats.files_created, 1);
        assert!(updated_stats.disk_usage > 0);
    }

    #[tokio::test]
    async fn test_empty_messages() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1h".to_string(),
        ).unwrap();

        manager.setup_rotation().await.unwrap();

        // Storing empty messages should not create files
        manager.store_messages(vec![]).await.unwrap();

        let stats = manager.get_storage_stats().await.unwrap();
        assert_eq!(stats.total_messages, 0);
        assert_eq!(stats.files_created, 0);
    }

    #[test]
    fn test_file_rotation_size_check() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "1KB".to_string(), // Very small size for testing
            "1h".to_string(),
        ).unwrap();

        let file_info = FileInfo {
            path: temp_dir.path().join("test.jsonl"),
            size: 2048, // 2KB, larger than rotation size
            created: Utc::now(),
            message_count: 10,
        };

        // Should rotate due to size
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let should_rotate = runtime.block_on(manager.should_rotate_file(&file_info));
        assert!(should_rotate);
    }

    #[test]
    fn test_file_rotation_time_check() {
        let temp_dir = tempdir().unwrap();
        let manager = FileStorageManager::new(
            temp_dir.path().to_path_buf(),
            "json".to_string(),
            "100MB".to_string(),
            "1s".to_string(), // Very short time for testing
        ).unwrap();

        let file_info = FileInfo {
            path: temp_dir.path().join("test.jsonl"),
            size: 100, // Small size
            created: Utc::now() - chrono::Duration::seconds(2), // 2 seconds ago
            message_count: 1,
        };

        // Should rotate due to age
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let should_rotate = runtime.block_on(manager.should_rotate_file(&file_info));
        assert!(should_rotate);
    }
}