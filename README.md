# Twitch Chat Scraper

This tool scrapes live chat messages from Twitch streams for analysis.

## Installation

You'll need Rust. Download from [rustup.rs](https://rustup.rs/).

Then:
1. Clone: `git clone github.com/rbxpusk/scrape-main`
2. Build: `cd scrape-main && cargo build --release`

## Usage

Run: `./target/release/scrape-main`

It uses `config.toml` for settings.

## Configuration

Edit `config.toml`:

```toml
[streamers]
streamers = ["shroud", "ninja"]

[agents]
max_concurrent = 5
delay_range = [1000, 5000]

[output]
directory = "./scraped_data"
format = "json"
rotation_size = "100MB"
rotation_time = "1h"

[monitoring]
tui_enabled = true
api_port = 8080
dashboard_port = 8888

[stealth]
randomize_user_agents = true
simulate_human_behavior = true
```

Save and restart.

## Notes

- Resource-heavy for many streams.
- Twitch site changes, might need updates.
- Be respectful of Twitch's terms.
- Check logs if issues.
