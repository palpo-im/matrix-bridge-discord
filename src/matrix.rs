use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tracing::{debug, info};

use crate::config::Config;

pub mod command_handler;
pub mod event_handler;

pub use self::command_handler::{
    MatrixCommandHandler, MatrixCommandOutcome, MatrixCommandPermission,
};
pub use self::event_handler::{MatrixEventHandler, MatrixEventHandlerImpl, MatrixEventProcessor};

#[derive(Clone)]
pub struct MatrixAppservice {
    config: Arc<Config>,
}

#[derive(Debug, Clone)]
pub struct MatrixEvent {
    pub event_id: Option<String>,
    pub event_type: String,
    pub room_id: String,
    pub sender: String,
    pub content: Option<Value>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MatrixRoom {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MatrixUser {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl MatrixAppservice {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!(
            "initializing matrix appservice for {}",
            config.bridge.domain
        );
        Ok(Self { config })
    }

    pub async fn start(&self) -> Result<()> {
        info!(
            "matrix appservice listener configured at {}:{}",
            self.config.bridge.bind_address, self.config.bridge.port
        );
        Ok(())
    }

    pub async fn create_ghost_user(
        &self,
        discord_user_id: &str,
        _username: &str,
        _display_name: Option<&str>,
    ) -> Result<String> {
        Ok(format!(
            "@_discord_{}:{}",
            discord_user_id, self.config.bridge.domain
        ))
    }

    pub async fn create_room(
        &self,
        discord_channel_id: &str,
        _name: &str,
        _topic: Option<&str>,
    ) -> Result<String> {
        Ok(format!(
            "#_discord_{}:{}",
            discord_channel_id, self.config.bridge.domain
        ))
    }

    pub async fn send_message(&self, room_id: &str, sender: &str, content: &str) -> Result<()> {
        debug!(
            "matrix send_message room={} sender={} content={}",
            room_id, sender, content
        );
        Ok(())
    }

    pub async fn send_notice(&self, room_id: &str, content: &str) -> Result<()> {
        debug!("matrix send_notice room={} body={}", room_id, content);
        Ok(())
    }

    pub async fn send_message_with_metadata(
        &self,
        room_id: &str,
        sender: &str,
        body: &str,
        attachments: &[String],
        reply_to: Option<&str>,
        edit_of: Option<&str>,
    ) -> Result<()> {
        debug!(
            "matrix send_message room={} sender={} reply_to={:?} edit_of={:?} attachments={} body={}",
            room_id,
            sender,
            reply_to,
            edit_of,
            attachments.len(),
            body
        );
        self.send_message(room_id, sender, body).await
    }

    pub async fn check_permission(
        &self,
        _user_id: &str,
        _room_id: &str,
        _required_level: i64,
        _category: &str,
        _subcategory: &str,
    ) -> Result<bool> {
        Ok(true)
    }

    pub async fn ensure_ghost_user_registered(
        &self,
        discord_user_id: &str,
        username: Option<&str>,
    ) -> Result<()> {
        let display = username.or(Some("Discord user"));
        self.create_ghost_user(
            discord_user_id,
            username.unwrap_or(discord_user_id),
            display,
        )
        .await?;
        Ok(())
    }

    pub async fn set_discord_user_presence(
        &self,
        discord_user_id: &str,
        presence: &str,
        status_message: &str,
    ) -> Result<()> {
        debug!(
            "matrix set_presence discord_user={} presence={} status={}",
            discord_user_id, presence, status_message
        );
        Ok(())
    }

    pub async fn set_room_alias(&self, room_id: &str, alias: &str) -> Result<()> {
        debug!("matrix set_room_alias room={} alias={}", room_id, alias);
        Ok(())
    }

    pub fn registration_preview(&self) -> Value {
        json!({
            "id": "discord_bridge",
            "url": format!("http://{}:{}", self.config.bridge.bind_address, self.config.bridge.port),
            "sender_localpart": "_discord_",
            "rate_limited": false,
        })
    }
}
