use std::sync::Arc;

use anyhow::{Context, Result};
use matrix_bot_sdk::{
    appservice::{Appservice, AppserviceHandler},
    client::{MatrixAuth, MatrixClient},
    models::CreateRoom,
};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use url::Url;

use crate::config::Config;

pub mod command_handler;
pub mod event_handler;

pub use self::command_handler::{
    MatrixCommandHandler, MatrixCommandOutcome, MatrixCommandPermission,
};
pub use self::event_handler::{MatrixEventHandler, MatrixEventHandlerImpl, MatrixEventProcessor};

pub struct BridgeAppserviceHandler {
    processor: Option<Arc<MatrixEventProcessor>>,
}

#[async_trait::async_trait]
impl AppserviceHandler for BridgeAppserviceHandler {
    async fn on_transaction(&self, _txn_id: &str, body: &Value) -> Result<()> {
        let Some(processor) = &self.processor else {
            return Ok(());
        };

        if let Some(events) = body.get("events").and_then(|v| v.as_array()) {
            for event in events {
                let Some(room_id) = event.get("room_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(sender) = event.get("sender").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(event_type) = event.get("type").and_then(|v| v.as_str()) else {
                    continue;
                };

                let matrix_event = MatrixEvent {
                    event_id: event
                        .get("event_id")
                        .and_then(|v| v.as_str())
                        .map(ToOwned::to_owned),
                    event_type: event_type.to_owned(),
                    room_id: room_id.to_owned(),
                    sender: sender.to_owned(),
                    state_key: event
                        .get("state_key")
                        .and_then(|v| v.as_str())
                        .map(ToOwned::to_owned),
                    content: event.get("content").cloned(),
                    timestamp: event.get("origin_server_ts").map(|v| v.to_string()),
                };

                if let Err(e) = processor.process_event(matrix_event).await {
                    error!("error processing event: {}", e);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct MatrixAppservice {
    config: Arc<Config>,
    pub appservice: Appservice,
    handler: Arc<RwLock<BridgeAppserviceHandler>>,
}

#[derive(Debug, Clone)]
pub struct MatrixEvent {
    pub event_id: Option<String>,
    pub event_type: String,
    pub room_id: String,
    pub sender: String,
    pub state_key: Option<String>,
    pub content: Option<Value>,
    pub timestamp: Option<String>,
}

impl MatrixAppservice {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!(
            "initializing matrix appservice for {}",
            config.bridge.domain
        );

        let homeserver_url = Url::parse(&config.bridge.homeserver_url)?;
        let auth = MatrixAuth::new(&config.registration.appservice_token);
        let client = MatrixClient::new(homeserver_url, auth);

        let handler = Arc::new(RwLock::new(BridgeAppserviceHandler { processor: None }));

        // Use a wrapper to bridge AppserviceHandler to our internal handler
        struct HandlerWrapper(Arc<RwLock<BridgeAppserviceHandler>>);
        #[async_trait::async_trait]
        impl AppserviceHandler for HandlerWrapper {
            async fn on_transaction(&self, txn_id: &str, body: &Value) -> Result<()> {
                self.0.read().await.on_transaction(txn_id, body).await
            }
        }

        let appservice = Appservice::new(
            &config.registration.homeserver_token,
            &config.registration.appservice_token,
            client,
        )
        .with_appservice_id(&config.registration.bridge_id)
        .with_handler(Arc::new(HandlerWrapper(handler.clone())));

        Ok(Self {
            config,
            appservice,
            handler,
        })
    }

    pub fn config(&self) -> Arc<Config> {
        self.config.clone()
    }

    pub fn bot_user_id(&self) -> String {
        format!("@_discord_:{}", self.config.bridge.domain)
    }

    pub async fn set_processor(&self, processor: Arc<MatrixEventProcessor>) {
        self.handler.write().await.processor = Some(processor);
    }

    pub async fn start(&self) -> Result<()> {
        info!("matrix appservice starting");
        Ok(())
    }

    pub async fn create_ghost_user(
        &self,
        discord_user_id: &str,
        _username: &str,
        display_name: Option<&str>,
    ) -> Result<String> {
        let localpart = format!("_discord_{}", discord_user_id);
        let user_id = format!("@{}:{}", localpart, self.config.bridge.domain);

        let ghost_client = self.appservice.client.clone();
        ghost_client
            .impersonate_user_id(Some(&user_id), None::<&str>)
            .await;

        let _ = ghost_client
            .password_register(&localpart, "", display_name)
            .await;

        if let Some(display) = display_name {
            let _ = ghost_client.set_display_name(display).await;
        }

        Ok(user_id)
    }

    pub async fn create_room(
        &self,
        discord_channel_id: &str,
        name: &str,
        topic: Option<&str>,
    ) -> Result<String> {
        let alias_localpart = format!("_discord_{}", discord_channel_id);

        let visibility = match self.config.room.default_visibility.to_lowercase().as_str() {
            "public" => Some("public".to_string()),
            _ => Some("private".to_string()),
        };

        let opt = CreateRoom {
            visibility,
            room_alias_name: Some(alias_localpart),
            name: Some(name.to_owned()),
            topic: topic.map(ToOwned::to_owned),
            ..Default::default()
        };

        let room_id = self.appservice.client.create_room(&opt).await?;
        Ok(room_id)
    }

    pub async fn send_message(&self, room_id: &str, sender: &str, content: &str) -> Result<()> {
        self.send_message_with_metadata(room_id, sender, content, &[], None, None)
            .await
            .map(|_| ())
    }

    pub async fn send_notice(&self, room_id: &str, content: &str) -> Result<()> {
        self.appservice.client.send_notice(room_id, content).await?;
        Ok(())
    }

    pub async fn send_message_with_metadata(
        &self,
        room_id: &str,
        sender: &str,
        body: &str,
        _attachments: &[String], // TODO: Implement attachment uploading
        reply_to: Option<&str>,
        edit_of: Option<&str>,
    ) -> Result<String> {
        let ghost_client = self.appservice.client.clone();
        ghost_client
            .impersonate_user_id(Some(sender), None::<&str>)
            .await;

        let mut content = json!({
            "msgtype": "m.text",
            "body": body,
        });

        if let Some(reply_id) = reply_to {
            content["m.relates_to"] = json!({
                "m.in_reply_to": {
                    "event_id": reply_id
                }
            });
        }

        if let Some(edit_event_id) = edit_of {
            content["m.new_content"] = json!({
                "msgtype": "m.text",
                "body": body,
            });
            content["m.relates_to"] = json!({
                "rel_type": "m.replace",
                "event_id": edit_event_id,
            });
            content["body"] = format!("* {body}").into();
        }

        let event_id = ghost_client
            .send_event(room_id, "m.room.message", &content)
            .await?;

        Ok(event_id)
    }

    pub async fn redact_message(
        &self,
        room_id: &str,
        event_id: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let content = json!({
            "redacts": event_id,
            "reason": reason.unwrap_or(""),
        });
        self.appservice
            .client
            .send_event(room_id, "m.room.redaction", &content)
            .await?;
        Ok(())
    }

    pub async fn check_permission(
        &self,
        user_id: &str,
        room_id: &str,
        required_level: i64,
        _category: &str,
        _subcategory: &str,
    ) -> Result<bool> {
        let power_levels = self
            .appservice
            .client
            .get_room_state_event(room_id, "m.room.power_levels", "")
            .await;

        match power_levels {
            Ok(pl) => {
                let user_level = pl
                    .get("users")
                    .and_then(|u| u.get(user_id))
                    .and_then(|v| v.as_i64())
                    .unwrap_or_else(|| {
                        pl.get("users_default")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                    });
                Ok(user_level >= required_level)
            }
            Err(_) => {
                // If we can't fetch power levels, default to denying
                Ok(false)
            }
        }
    }

    pub async fn ensure_ghost_user_registered(
        &self,
        discord_user_id: &str,
        username: Option<&str>,
    ) -> Result<()> {
        let display = username.unwrap_or(discord_user_id);
        self.create_ghost_user(discord_user_id, display, Some(display))
            .await?;
        Ok(())
    }

    pub async fn set_discord_user_presence(
        &self,
        discord_user_id: &str,
        presence: &str,
        status_message: &str,
    ) -> Result<()> {
        let localpart = format!("_discord_{}", discord_user_id);
        let user_id = format!("@{}:{}", localpart, self.config.bridge.domain);

        let ghost_client = self.appservice.client.clone();
        ghost_client
            .impersonate_user_id(Some(&user_id), None::<&str>)
            .await;

        let presence_status = match presence {
            "online" => matrix_bot_sdk::models::Presence::Online,
            "unavailable" => matrix_bot_sdk::models::Presence::Unavailable,
            _ => matrix_bot_sdk::models::Presence::Offline,
        };

        ghost_client
            .set_presence_status(presence_status, Some(status_message))
            .await?;
        Ok(())
    }

    pub async fn set_room_alias(&self, room_id: &str, alias: &str) -> Result<()> {
        self.appservice
            .client
            .create_room_alias(alias, room_id)
            .await?;
        Ok(())
    }

    pub fn registration_preview(&self) -> Value {
        json!({
            "id": self.config.registration.bridge_id,
            "url": format!("http://{}:{}", self.config.bridge.bind_address, self.config.bridge.port),
            "as_token": self.config.registration.appservice_token,
            "hs_token": self.config.registration.homeserver_token,
            "sender_localpart": "_discord_",
            "rate_limited": false,
            "namespaces": {
                "users": [{
                    "exclusive": true,
                    "regex": format!("@_discord_.*:{}", self.config.bridge.domain)
                }],
                "aliases": [{
                    "exclusive": true,
                    "regex": format!("#_discord_.*:{}", self.config.bridge.domain)
                }],
                "rooms": []
            }
        })
    }
}
