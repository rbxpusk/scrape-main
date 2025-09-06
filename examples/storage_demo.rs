use chrono::Utc;
use std::path::PathBuf;
use tempfile::tempdir;
use twitch_chat_scraper::parser::chat_message::{
    ChatMessage, ChatUser, MessageContent, MessageFragment, StreamContext,
};
use twitch_chat_scraper::storage::{FileStorageManager, StorageManager};
use twitch_chat_scraper::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("🗄️  Storage Manager Demo");
    println!("========================");

    // creating temp dir for demo
    let temp_dir = tempdir()?;
    println!("📁 Using temporary directory: {:?}", temp_dir.path());

    // setting up json storage
    let json_manager = FileStorageManager::new(
        temp_dir.path().join("json_output"),
        "json".to_string(),
        "1MB".to_string(),
        "1h".to_string(),
    )?;

    // setting up csv storage
    let csv_manager = FileStorageManager::new(
        temp_dir.path().join("csv_output"),
        "csv".to_string(),
        "1MB".to_string(),
        "1h".to_string(),
    )?;

    // setting up rotation for both
    json_manager.setup_rotation().await?;
    csv_manager.setup_rotation().await?;

    println!("✅ Storage managers initialized");

    // creating sample messages
    let messages = vec![
        ChatMessage::new(
            "shroud".to_string(),
            Utc::now(),
            ChatUser {
                username: "viewer1".to_string(),
                display_name: "Viewer1".to_string(),
                color: Some("#FF0000".to_string()),
                badges: vec!["subscriber".to_string(), "vip".to_string()],
            },
            MessageContent {
                text: "Great gameplay!".to_string(),
                emotes: vec![],
                fragments: vec![MessageFragment {
                    fragment_type: "text".to_string(),
                    content: "Great gameplay!".to_string(),
                }],
            },
            StreamContext {
                viewer_count: Some(15000),
                game_category: Some("Valorant".to_string()),
                stream_title: Some("Ranked Grind".to_string()),
            },
        ),
        ChatMessage::new(
            "shroud".to_string(),
            Utc::now(),
            ChatUser {
                username: "viewer2".to_string(),
                display_name: "Viewer2".to_string(),
                color: Some("#00FF00".to_string()),
                badges: vec!["moderator".to_string()],
            },
            MessageContent {
                text: "Nice shot!".to_string(),
                emotes: vec!["Kappa".to_string()],
                fragments: vec![
                    MessageFragment {
                        fragment_type: "text".to_string(),
                        content: "Nice shot! ".to_string(),
                    },
                    MessageFragment {
                        fragment_type: "emote".to_string(),
                        content: "Kappa".to_string(),
                    },
                ],
            },
            StreamContext {
                viewer_count: Some(15000),
                game_category: Some("Valorant".to_string()),
                stream_title: Some("Ranked Grind".to_string()),
            },
        ),
        ChatMessage::new(
            "ninja".to_string(),
            Utc::now(),
            ChatUser {
                username: "fan123".to_string(),
                display_name: "Fan123".to_string(),
                color: Some("#0000FF".to_string()),
                badges: vec!["subscriber".to_string()],
            },
            MessageContent {
                text: "Hello from another stream!".to_string(),
                emotes: vec![],
                fragments: vec![MessageFragment {
                    fragment_type: "text".to_string(),
                    content: "Hello from another stream!".to_string(),
                }],
            },
            StreamContext {
                viewer_count: Some(8000),
                game_category: Some("Fortnite".to_string()),
                stream_title: Some("Victory Royale Hunt".to_string()),
            },
        ),
    ];

    println!("📝 Created {} sample messages", messages.len());

    // storing messages as json
    println!("💾 Storing messages in JSON format...");
    json_manager.store_messages(messages.clone()).await?;

    // storing messages as csv
    println!("💾 Storing messages in CSV format...");
    csv_manager.store_messages(messages).await?;

    // getting storage stats
    let json_stats = json_manager.get_storage_stats().await?;
    let csv_stats = csv_manager.get_storage_stats().await?;

    println!("\n📊 Storage Statistics:");
    println!("JSON Storage:");
    println!("  - Total messages: {}", json_stats.total_messages);
    println!("  - Files created: {}", json_stats.files_created);
    println!("  - Disk usage: {} bytes", json_stats.disk_usage);

    println!("CSV Storage:");
    println!("  - Total messages: {}", csv_stats.total_messages);
    println!("  - Files created: {}", csv_stats.files_created);
    println!("  - Disk usage: {} bytes", csv_stats.disk_usage);

    // showing directory structure
    println!("\n📂 Directory Structure:");
    show_directory_structure(temp_dir.path(), 0)?;

    // showing sample file contents
    println!("\n📄 Sample File Contents:");
    show_sample_files(temp_dir.path()).await?;

    println!("\n✅ Storage demo completed successfully!");

    Ok(())
}

fn show_directory_structure(path: &std::path::Path, depth: usize) -> Result<()> {
    let indent = "  ".repeat(depth);
    
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            
            if entry.path().is_dir() {
                println!("{}📁 {}/", indent, file_name_str);
                if depth < 3 {  // Limit recursion depth
                    show_directory_structure(&entry.path(), depth + 1)?;
                }
            } else {
                let metadata = entry.metadata()?;
                println!("{}📄 {} ({} bytes)", indent, file_name_str, metadata.len());
            }
        }
    }
    
    Ok(())
}

async fn show_sample_files(base_path: &std::path::Path) -> Result<()> {
    // Find and show JSON file content
    if let Some(json_file) = find_file_with_extension(base_path, "jsonl")? {
        println!("\n📄 JSON File Content ({})", json_file.file_name().unwrap().to_string_lossy());
        println!("---");
        let content = std::fs::read_to_string(&json_file)?;
        for (i, line) in content.lines().take(2).enumerate() {
            if !line.trim().is_empty() {
                println!("Line {}: {}", i + 1, line);
            }
        }
    }

    // Find and show CSV file content
    if let Some(csv_file) = find_file_with_extension(base_path, "csv")? {
        println!("\n📄 CSV File Content ({})", csv_file.file_name().unwrap().to_string_lossy());
        println!("---");
        let content = std::fs::read_to_string(&csv_file)?;
        for (i, line) in content.lines().take(4).enumerate() {
            if !line.trim().is_empty() {
                println!("Line {}: {}", i + 1, line);
            }
        }
    }

    Ok(())
}

fn find_file_with_extension(
    dir: &std::path::Path,
    extension: &str,
) -> Result<Option<PathBuf>> {
    fn search_recursive(dir: &std::path::Path, extension: &str) -> Result<Option<PathBuf>> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == extension {
                            return Ok(Some(path));
                        }
                    }
                } else if path.is_dir() {
                    if let Some(found) = search_recursive(&path, extension)? {
                        return Ok(Some(found));
                    }
                }
            }
        }
        Ok(None)
    }
    
    search_recursive(dir, extension)
}