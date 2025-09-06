use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A fragment of a chat message, either text or emote
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageFragment {
    #[serde(rename = "type")]
    pub fragment_type: String,
    pub content: String,
}

/// User info pulled from the chat message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatUser {
    pub username: String,
    pub display_name: String,
    pub color: Option<String>,
    pub badges: Vec<String>,
}

/// Message content with text and emotes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageContent {
    pub text: String,
    pub emotes: Vec<String>,
    pub fragments: Vec<MessageFragment>,
}

/// Context about the stream
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamContext {
    pub viewer_count: Option<u32>,
    pub game_category: Option<String>,
    pub stream_title: Option<String>,
}

/// Full chat message setup for LLM training
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub streamer: String,
    pub timestamp: DateTime<Utc>,
    pub user: ChatUser,
    pub message: MessageContent,
    pub context: StreamContext,
}

impl ChatMessage {
    // make a new chatmessage with an id
    pub fn new(
        streamer: String,
        timestamp: DateTime<Utc>,
        user: ChatUser,
        message: MessageContent,
        context: StreamContext,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        
        Self {
            id,
            streamer,
            timestamp,
            user,
            message,
            context,
        }
    }

    // create a hash for the content to spot duplicates
    pub fn content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.streamer.as_bytes());
        hasher.update(self.user.username.as_bytes());
        hasher.update(self.message.text.as_bytes());
        hasher.update(self.timestamp.timestamp().to_string().as_bytes());
        
        format!("{:x}", hasher.finalize())
    }

    // check if the message looks good and has what we need
    pub fn is_valid(&self) -> bool {
        !self.user.username.is_empty() 
            && !self.message.text.is_empty() 
            && !self.streamer.is_empty()
    }

    // how long the message is in characters
    pub fn message_length(&self) -> usize {
        self.message.text.len()
    }

    // simple check if this might be spam
    pub fn is_likely_spam(&self) -> bool {
        let text = &self.message.text;
        
        // Check for excessive repetition
        if text.len() > 10 {
            let unique_chars: std::collections::HashSet<char> = text.chars().collect();
            if unique_chars.len() < text.len() / 4 {
                return true;
            }
        }
        
        // Check for excessive caps
        let caps_count = text.chars().filter(|c| c.is_uppercase()).count();
        if text.len() > 5 && caps_count > text.len() * 3 / 4 {
            return true;
        }
        
        // Check for excessive special characters
        let special_count = text.chars().filter(|c| !c.is_alphanumeric() && !c.is_whitespace()).count();
        if special_count > text.len() / 2 {
            return true;
        }
        
        false
    }
}

impl Default for StreamContext {
    fn default() -> Self {
        Self {
            viewer_count: None,
            game_category: None,
            stream_title: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_message() -> ChatMessage {
        ChatMessage::new(
            "teststreamer".to_string(),
            Utc::now(),
            ChatUser {
                username: "testuser".to_string(),
                display_name: "TestUser".to_string(),
                color: Some("#FF0000".to_string()),
                badges: vec!["subscriber".to_string()],
            },
            MessageContent {
                text: "Hello world!".to_string(),
                emotes: vec![],
                fragments: vec![MessageFragment {
                    fragment_type: "text".to_string(),
                    content: "Hello world!".to_string(),
                }],
            },
            StreamContext::default(),
        )
    }

    #[test]
    fn test_message_creation() {
        let message = create_test_message();
        assert!(!message.id.is_empty());
        assert_eq!(message.streamer, "teststreamer");
        assert_eq!(message.user.username, "testuser");
        assert_eq!(message.message.text, "Hello world!");
    }

    #[test]
    fn test_message_validation() {
        let valid_message = create_test_message();
        assert!(valid_message.is_valid());

        let mut invalid_message = create_test_message();
        invalid_message.user.username = "".to_string();
        assert!(!invalid_message.is_valid());
    }

    #[test]
    fn test_content_hash() {
        let message1 = create_test_message();
        let message2 = create_test_message();
        
        // Same content should produce same hash
        assert_eq!(message1.content_hash(), message2.content_hash());
    }

    #[test]
    fn test_spam_detection() {
        let normal_message = create_test_message();
        assert!(!normal_message.is_likely_spam());

        let mut spam_message = create_test_message();
        spam_message.message.text = "AAAAAAAAAAAAAAAA".to_string();
        assert!(spam_message.is_likely_spam());

        let mut caps_spam = create_test_message();
        caps_spam.message.text = "THIS IS ALL CAPS SPAM MESSAGE".to_string();
        assert!(caps_spam.is_likely_spam());
    }

    #[test]
    fn test_serialization() {
        let message = create_test_message();
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(message, deserialized);
    }
}