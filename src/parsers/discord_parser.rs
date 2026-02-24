use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;

use crate::discord::DiscordClient;

use super::common::{BridgeMessage, MessageUtils, ParsedMessage};

pub struct DiscordMessageParser {
    _client: Arc<DiscordClient>,
}

impl DiscordMessageParser {
    pub fn new(client: Arc<DiscordClient>) -> Self {
        Self { _client: client }
    }

    pub fn parse_message(&self, content: &str) -> ParsedMessage {
        ParsedMessage::new(content)
    }
}

pub struct DiscordToMatrixConverter {
    _discord_client: Arc<DiscordClient>,
}

impl DiscordToMatrixConverter {
    pub fn new(discord_client: Arc<DiscordClient>) -> Self {
        Self {
            _discord_client: discord_client,
        }
    }

    pub fn format_for_matrix(&self, message: &str) -> String {
        MessageUtils::sanitize_markdown(message)
    }

    pub async fn convert_message(
        &self,
        discord_message: &str,
        matrix_room_id: &str,
    ) -> Result<BridgeMessage> {
        Ok(BridgeMessage {
            source_platform: "discord".to_string(),
            target_platform: "matrix".to_string(),
            source_id: format!("discord:{}", matrix_room_id),
            target_id: matrix_room_id.to_string(),
            content: self.format_for_matrix(discord_message),
            timestamp: Utc::now().to_rfc3339(),
            attachments: Vec::new(),
        })
    }
}
