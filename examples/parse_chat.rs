use twitch_chat_scraper::{DataProcessor, TwitchChatParser};

const SAMPLE_TWITCH_HTML: &str = r#"
<div class="Layout-sc-1xcs6mc-0 fHdBNk chat-line__no-background">
    <div class="Layout-sc-1xcs6mc-0 dtoOxd">
        <div class="Layout-sc-1xcs6mc-0 nnbce chat-line__username-container">
            <span class="chat-line__username" role="button" tabindex="0">
                <span class="chat-author__display-name" 
                      data-a-target="chat-message-username" 
                      data-a-user="nauviel" 
                      style="color: rgb(154, 205, 50);">Nauviel</span>
            </span>
        </div>
        <span aria-hidden="true">: </span>
        <span data-a-target="chat-line-message-body" dir="auto">
            <span class="text-fragment" data-a-target="chat-message-text">skillshot lander missed</span>
        </span>
    </div>
</div>

<div class="Layout-sc-1xcs6mc-0 fHdBNk chat-line__no-background">
    <div class="Layout-sc-1xcs6mc-0 dtoOxd">
        <div class="Layout-sc-1xcs6mc-0 nnbce chat-line__username-container">
            <span class="chat-line__username" role="button" tabindex="0">
                <span class="chat-author__display-name" 
                      data-a-target="chat-message-username" 
                      data-a-user="chatuser123" 
                      style="color: rgb(255, 0, 0);">ChatUser123</span>
            </span>
        </div>
        <span aria-hidden="true">: </span>
        <span data-a-target="chat-line-message-body" dir="auto">
            <span class="text-fragment">Nice play! </span>
            <img class="chat-line__message--emote" alt="Kappa" src="emote.png">
        </span>
    </div>
</div>
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Twitch Chat Parser Example ===\n");

    // let's create a parser
    let parser = TwitchChatParser::new()?;
    println!("✓ Created TwitchChatParser");

    let messages = parser.parse_chat_html(SAMPLE_TWITCH_HTML, "examplestreamer")?;
    println!("✓ Parsed {} messages from HTML", messages.len());

    // showing the parsed messages
    for (i, message) in messages.iter().enumerate() {
        println!("\n--- Message {} ---", i + 1);
        println!("User: {} ({})", message.user.display_name, message.user.username);
        if let Some(color) = &message.user.color {
            println!("Color: {}", color);
        }
        println!("Text: {}", message.message.text);
        println!("Fragments: {} text, {} emotes", 
                 message.message.fragments.iter().filter(|f| f.fragment_type == "text").count(),
                 message.message.fragments.iter().filter(|f| f.fragment_type == "emote").count());
        if !message.message.emotes.is_empty() {
            println!("Emotes: {:?}", message.message.emotes);
        }
        println!("Timestamp: {}", message.timestamp);
        println!("Content Hash: {}", message.content_hash());
    }

    println!("\n=== Data Processor Example ===\n");

    // setting up a data processor with some filters
    let mut processor = DataProcessor::with_settings(5, 100, true, true)?;
    println!("✓ Created DataProcessor with filtering enabled");

    let filtered_messages = processor.process_html(SAMPLE_TWITCH_HTML, "examplestreamer")?;
    println!("✓ Processed and filtered {} messages", filtered_messages.len());

    // here are the filtered results
    for (i, message) in filtered_messages.iter().enumerate() {
        println!("\n--- Filtered Message {} ---", i + 1);
        println!("User: {}", message.user.username);
        println!("Text: {}", message.message.text);
        println!("Valid: {}", message.is_valid());
        println!("Likely Spam: {}", message.is_likely_spam());
    }

    println!("\n=== JSON Serialization Example ===\n");

    // turning it into json for llm training
    if let Some(message) = filtered_messages.first() {
        let json = serde_json::to_string_pretty(message)?;
        println!("Sample message in LLM training format:");
        println!("{}", json);
    }

    println!("\n✓ Chat parsing example completed successfully!");

    Ok(())
}