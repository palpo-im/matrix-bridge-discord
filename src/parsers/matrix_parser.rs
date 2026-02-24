use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;

use crate::matrix::{MatrixAppservice, MatrixEvent};

use super::common::{BridgeMessage, MessageUtils, ParsedMessage};

pub struct MatrixMessageParser {
    _client: Arc<MatrixAppservice>,
}

impl MatrixMessageParser {
    pub fn new(client: Arc<MatrixAppservice>) -> Self {
        Self { _client: client }
    }

    pub fn parse_message(&self, content: &str) -> ParsedMessage {
        ParsedMessage::new(content)
    }
}

pub struct MatrixToDiscordConverter {
    _matrix_client: Arc<MatrixAppservice>,
}

impl MatrixToDiscordConverter {
    pub fn new(matrix_client: Arc<MatrixAppservice>) -> Self {
        Self {
            _matrix_client: matrix_client,
        }
    }

    pub fn format_for_discord(&self, message: &str) -> String {
        message.to_string()
    }

    pub async fn convert_message(
        &self,
        matrix_event: &MatrixEvent,
        discord_channel_id: &str,
    ) -> Result<BridgeMessage> {
        let plain = matrix_event
            .content
            .as_ref()
            .map(MessageUtils::extract_plain_text)
            .unwrap_or_default();

        Ok(BridgeMessage {
            source_platform: "matrix".to_string(),
            target_platform: "discord".to_string(),
            source_id: format!("{}:{}", matrix_event.room_id, matrix_event.sender),
            target_id: discord_channel_id.to_string(),
            content: self.format_for_discord(&plain),
            timestamp: matrix_event
                .timestamp
                .clone()
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
            attachments: Vec::new(),
        })
    }
}
