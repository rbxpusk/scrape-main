use crate::error::ScrapingError;
use crate::parser::{ChatMessage, ChatUser, MessageContent, MessageFragment, StreamContext};
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use tracing::{debug, warn};

/// html parser for pulling twitch chat messages
pub struct TwitchChatParser {
    // CSS selectors for different parts of chat messages
    chat_line_selector: Selector,
    username_selector: Selector,
    display_name_selector: Selector,
    message_body_selector: Selector,
    badge_selector: Selector,
    timestamp_selector: Selector,
}

impl TwitchChatParser {
    // set up a parser with css selectors ready
    pub fn new() -> Result<Self, ScrapingError> {
        Ok(Self {
            chat_line_selector: Selector::parse(".chat-line__no-background, .chat-line__message")
                .map_err(|e| ScrapingError::ParseError(format!("Invalid chat line selector: {}", e)))?,
            username_selector: Selector::parse("[data-a-target='chat-message-username']")
                .map_err(|e| ScrapingError::ParseError(format!("Invalid username selector: {}", e)))?,
            display_name_selector: Selector::parse(".chat-author__display-name")
                .map_err(|e| ScrapingError::ParseError(format!("Invalid display name selector: {}", e)))?,
            message_body_selector: Selector::parse("[data-a-target='chat-line-message-body']")
                .map_err(|e| ScrapingError::ParseError(format!("Invalid message body selector: {}", e)))?,
            badge_selector: Selector::parse(".chat-badge")
                .map_err(|e| ScrapingError::ParseError(format!("Invalid badge selector: {}", e)))?,
            timestamp_selector: Selector::parse(".chat-line__timestamp")
                .map_err(|e| ScrapingError::ParseError(format!("Invalid timestamp selector: {}", e)))?,
        })
    }

    // pull chat messages from html
    pub fn parse_chat_html(&self, html: &str, streamer: &str) -> Result<Vec<ChatMessage>, ScrapingError> {
        let document = Html::parse_document(html);
        let mut messages = Vec::new();

        for chat_line in document.select(&self.chat_line_selector) {
            match self.parse_single_message(&chat_line, streamer) {
                Ok(Some(message)) => {
                    if message.is_valid() {
                        messages.push(message);
                    } else {
                        debug!("Skipping invalid message: {:?}", message);
                    }
                }
                Ok(None) => {
                    debug!("Skipped message (likely system message or empty)");
                }
                Err(e) => {
                    warn!("Failed to parse message: {}", e);
                    // Continue processing other messages instead of failing completely
                }
            }
        }

        debug!("Parsed {} messages from HTML", messages.len());
        Ok(messages)
    }

    // handle one chat message element
    fn parse_single_message(
        &self,
        element: &scraper::ElementRef,
        streamer: &str,
    ) -> Result<Option<ChatMessage>, ScrapingError> {
        // Extract user information
        let user = match self.extract_user_info(element) {
            Ok(Some(user)) => user,
            Ok(None) => return Ok(None), // System message or deleted message
            Err(e) => return Err(e),
        };

        // Extract message content
        let message_content = self.extract_message_content(element)?;
        
        // Skip empty messages
        if message_content.text.trim().is_empty() {
            return Ok(None);
        }

        // Extract timestamp (use current time if not found)
        let timestamp = self.extract_timestamp(element).unwrap_or_else(Utc::now);

        // Create stream context (basic for now)
        let context = StreamContext::default();

        let message = ChatMessage::new(
            streamer.to_string(),
            timestamp,
            user,
            message_content,
            context,
        );

        Ok(Some(message))
    }

    // grab user info from the chat message element
    fn extract_user_info(&self, element: &scraper::ElementRef) -> Result<Option<ChatUser>, ScrapingError> {
        // Try to find username element
        let username_element = match element.select(&self.username_selector).next() {
            Some(elem) => elem,
            None => {
                debug!("No username found, likely system message");
                return Ok(None);
            }
        };

        // Extract username from data attribute or text content
        let username = if let Some(data_user) = username_element.value().attr("data-a-user") {
            data_user.to_string()
        } else if let Some(display_elem) = username_element.select(&self.display_name_selector).next() {
            let text = display_elem.text().collect::<String>();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                return Err(ScrapingError::ParseError("Empty display name".to_string()));
            }
        } else {
            let text = username_element.text().collect::<String>();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                return Err(ScrapingError::ParseError("Could not extract username".to_string()));
            }
        };

        // Extract display name (may be different from username)
        let display_name = element
            .select(&self.display_name_selector)
            .next()
            .map(|elem| elem.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| username.clone());

        // Extract user color from style attribute
        let color = element
            .select(&self.display_name_selector)
            .next()
            .and_then(|elem| elem.value().attr("style"))
            .and_then(|style| self.extract_color_from_style(style));

        // Extract badges
        let badges = self.extract_badges(element);

        Ok(Some(ChatUser {
            username,
            display_name,
            color,
            badges,
        }))
    }

    // pull out message content with text and emotes
    fn extract_message_content(&self, element: &scraper::ElementRef) -> Result<MessageContent, ScrapingError> {
        let message_body = element
            .select(&self.message_body_selector)
            .next()
            .ok_or_else(|| ScrapingError::ParseError("Could not find message body".to_string()))?;

        let mut text_parts = Vec::new();
        let mut fragments = Vec::new();
        let mut emotes = Vec::new();

        // Collect all elements (text and emotes) in document order
        let all_selector = Selector::parse("span.text-fragment, span[data-a-target='chat-message-text'], img").unwrap();
        
        for elem in message_body.select(&all_selector) {
            if elem.value().name() == "img" && 
               (elem.value().classes().any(|c| c == "chat-line__message--emote") ||
                elem.value().classes().any(|c| c == "chat-image")) {
                // It's an emote
                if let Some(alt_text) = elem.value().attr("alt") {
                    emotes.push(alt_text.to_string());
                    fragments.push(MessageFragment {
                        fragment_type: "emote".to_string(),
                        content: alt_text.to_string(),
                    });
                    text_parts.push(alt_text.to_string());
                }
            } else if elem.value().name() == "span" {
                // It's a text fragment
                let text = elem.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    text_parts.push(text.clone());
                    fragments.push(MessageFragment {
                        fragment_type: "text".to_string(),
                        content: text,
                    });
                }
            }
        }

        // If no fragments found, try to get all text content
        if fragments.is_empty() {
            let text = message_body.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                text_parts.push(text.clone());
                fragments.push(MessageFragment {
                    fragment_type: "text".to_string(),
                    content: text,
                });
            }
        }

        // Join text parts with spaces and clean up extra whitespace
        let full_text = text_parts.join(" ")
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ");

        Ok(MessageContent {
            text: full_text,
            emotes,
            fragments,
        })
    }

    // get timestamp from the message element
    fn extract_timestamp(&self, element: &scraper::ElementRef) -> Option<DateTime<Utc>> {
        element
            .select(&self.timestamp_selector)
            .next()
            .and_then(|elem| elem.value().attr("datetime"))
            .and_then(|datetime_str| DateTime::parse_from_rfc3339(datetime_str).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    // pull color from css style
    fn extract_color_from_style(&self, style: &str) -> Option<String> {
        // Look for color: rgb(...) or color: #...
        if let Some(color_start) = style.find("color:") {
            let color_part = &style[color_start + 6..];
            
            // Handle rgb() format
            if let Some(rgb_start) = color_part.find("rgb(") {
                if let Some(rgb_end) = color_part[rgb_start..].find(')') {
                    let rgb_values = &color_part[rgb_start + 4..rgb_start + rgb_end];
                    if let Ok(hex) = self.rgb_to_hex(rgb_values) {
                        return Some(hex);
                    }
                }
            }
            
            // Handle hex format
            if let Some(hex_start) = color_part.find('#') {
                let hex_part = &color_part[hex_start..];
                if let Some(hex_end) = hex_part.find(';').or_else(|| hex_part.find(' ')) {
                    return Some(hex_part[..hex_end].to_string());
                } else if hex_part.len() >= 7 {
                    return Some(hex_part[..7].to_string());
                }
            }
        }
        
        None
    }

    // turn rgb values into hex color
    fn rgb_to_hex(&self, rgb_str: &str) -> Result<String, ScrapingError> {
        let values: Result<Vec<u8>, _> = rgb_str
            .split(',')
            .map(|s| s.trim().parse::<u8>())
            .collect();

        match values {
            Ok(rgb) if rgb.len() == 3 => Ok(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])),
            _ => Err(ScrapingError::ParseError(format!("Invalid RGB values: {}", rgb_str))),
        }
    }

    // grab user badges from the message
    fn extract_badges(&self, element: &scraper::ElementRef) -> Vec<String> {
        element
            .select(&self.badge_selector)
            .filter_map(|badge| {
                badge.value().attr("alt")
                    .or_else(|| badge.value().attr("title"))
                    .map(|s| s.to_string())
            })
            .collect()
    }
}

impl Default for TwitchChatParser {
    fn default() -> Self {
        Self::new().expect("Failed to create default TwitchChatParser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOCK_CHAT_HTML: &str = r#"
    <div class="Layout-sc-1xcs6mc-0 fHdBNk chat-line__no-background">
        <div class="Layout-sc-1xcs6mc-0 dtoOxd">
            <div class="Layout-sc-1xcs6mc-0 nnbce chat-line__username-container">
                <span class="chat-line__username" role="button" tabindex="0">
                    <span class="chat-author__display-name" 
                          data-a-target="chat-message-username" 
                          data-a-user="testuser" 
                          style="color: rgb(154, 205, 50);">TestUser</span>
                </span>
            </div>
            <span aria-hidden="true">: </span>
            <span data-a-target="chat-line-message-body" dir="auto">
                <span class="text-fragment" data-a-target="chat-message-text">Test message content</span>
            </span>
        </div>
    </div>
    "#;

    const MOCK_CHAT_WITH_EMOTE: &str = r#"
    <div class="chat-line__no-background">
        <div>
            <span data-a-target="chat-message-username" data-a-user="emoteuser">EmoteUser</span>
            <span data-a-target="chat-line-message-body">
                <span class="text-fragment">Hello </span>
                <img class="chat-line__message--emote" alt="Kappa" src="emote.png">
                <span class="text-fragment"> world</span>
            </span>
        </div>
    </div>
    "#;

    #[test]
    fn test_parser_creation() {
        let parser = TwitchChatParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_basic_message() {
        let parser = TwitchChatParser::new().unwrap();
        let messages = parser.parse_chat_html(MOCK_CHAT_HTML, "teststreamer").unwrap();
        
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        
        assert_eq!(message.user.username, "testuser");
        assert_eq!(message.user.display_name, "TestUser");
        assert_eq!(message.message.text, "Test message content");
        assert_eq!(message.streamer, "teststreamer");
        assert!(message.is_valid());
    }

    #[test]
    fn test_parse_message_with_emote() {
        let parser = TwitchChatParser::new().unwrap();
        let messages = parser.parse_chat_html(MOCK_CHAT_WITH_EMOTE, "teststreamer").unwrap();
        
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        
        assert_eq!(message.user.username, "emoteuser");
        assert_eq!(message.message.text, "Hello Kappa world");
        assert_eq!(message.message.emotes, vec!["Kappa"]);
        assert_eq!(message.message.fragments.len(), 3);
    }

    #[test]
    fn test_color_extraction() {
        let parser = TwitchChatParser::new().unwrap();
        
        // Test RGB to hex conversion
        assert_eq!(parser.rgb_to_hex("154, 205, 50").unwrap(), "#9ACD32");
        
        // Test style parsing
        assert_eq!(
            parser.extract_color_from_style("color: rgb(154, 205, 50);"),
            Some("#9ACD32".to_string())
        );
        
        assert_eq!(
            parser.extract_color_from_style("color: #FF0000;"),
            Some("#FF0000".to_string())
        );
    }

    #[test]
    fn test_empty_html() {
        let parser = TwitchChatParser::new().unwrap();
        let messages = parser.parse_chat_html("", "teststreamer").unwrap();
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_malformed_html() {
        let parser = TwitchChatParser::new().unwrap();
        let malformed_html = "<div><span>incomplete";
        let messages = parser.parse_chat_html(malformed_html, "teststreamer").unwrap();
        // Should not crash, may return empty or partial results
        assert!(messages.len() >= 0);
    }
}