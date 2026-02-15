use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedMessage {
    pub text: String,
    pub formatted: Option<String>,
    pub mentions: Vec<String>,
    pub attachments: Vec<String>,
}

impl ParsedMessage {
    pub fn new(content: &str) -> Self {
        Self {
            text: content.to_string(),
            formatted: None,
            mentions: MessageUtils::extract_discord_mentions(content),
            attachments: MessageUtils::extract_discord_attachments(content),
        }
    }

    pub fn formatted_or_text(&self) -> String {
        self.formatted.clone().unwrap_or_else(|| self.text.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub source_platform: String,
    pub target_platform: String,
    pub source_id: String,
    pub target_id: String,
    pub content: String,
    pub timestamp: String,
    pub attachments: Vec<String>,
}

pub struct MessageUtils;

impl MessageUtils {
    pub fn sanitize_markdown(text: &str) -> String {
        text.replace('*', "\\*")
            .replace('_', "\\_")
            .replace('~', "\\~")
    }

    pub fn extract_plain_text(content: &Value) -> String {
        match content {
            Value::String(s) => s.clone(),
            Value::Object(obj) => {
                if let Some(body) = obj.get("body").and_then(Value::as_str) {
                    body.to_string()
                } else if let Some(formatted) = obj.get("formatted_body").and_then(Value::as_str) {
                    formatted.to_string()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        }
    }

    pub fn extract_discord_mentions(content: &str) -> Vec<String> {
        content
            .split_whitespace()
            .filter(|part| part.starts_with("<@") && part.ends_with('>'))
            .map(|s| s.to_string())
            .collect()
    }

    pub fn extract_discord_attachments(content: &str) -> Vec<String> {
        content
            .split_whitespace()
            .filter(|part| part.starts_with("http://") || part.starts_with("https://"))
            .map(|s| s.to_string())
            .collect()
    }
}
