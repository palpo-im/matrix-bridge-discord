use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::config::Config;

pub mod command_handler;

pub use self::command_handler::{DiscordCommandHandler, DiscordCommandOutcome, ModerationAction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordChannel {
    pub id: String,
    pub name: String,
    pub guild_id: String,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub content: String,
    pub attachments: Vec<String>,
    pub reply_to: Option<String>,
    pub edit_of: Option<String>,
    pub timestamp: String,
}

#[derive(Clone)]
pub struct DiscordClient {
    _config: Arc<Config>,
}

impl DiscordClient {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!("Initializing Discord client");
        Ok(Self { _config: config })
    }

    pub async fn start(&self) -> Result<()> {
        info!("Discord client is ready");
        Ok(())
    }

    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<String> {
        info!("Forwarding message to Discord channel {}", channel_id);
        Ok(format!("mock:{}:{}", channel_id, content.len()))
    }

    pub async fn send_message_with_metadata(
        &self,
        channel_id: &str,
        content: &str,
        attachments: &[String],
        reply_to: Option<&str>,
        edit_of: Option<&str>,
    ) -> Result<String> {
        debug!(
            "Discord send channel={} reply_to={:?} edit_of={:?} attachments={} content={}",
            channel_id,
            reply_to,
            edit_of,
            attachments.len(),
            content
        );
        self.send_message(channel_id, content).await
    }

    pub async fn get_user(&self, user_id: &str) -> Result<Option<DiscordUser>> {
        Ok(Some(DiscordUser {
            id: user_id.to_string(),
            username: format!("user_{}", user_id),
            discriminator: "0001".to_string(),
            avatar: None,
        }))
    }

    pub async fn get_channel(&self, channel_id: &str) -> Result<Option<DiscordChannel>> {
        Ok(Some(DiscordChannel {
            id: channel_id.to_string(),
            name: format!("channel_{}", channel_id),
            guild_id: "unknown_guild".to_string(),
            topic: None,
        }))
    }
}
