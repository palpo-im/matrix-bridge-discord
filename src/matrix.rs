use std::sync::Arc;

use anyhow::{Context, Result};
use matrix_bot_sdk::{
    appservice::{Appservice, AppserviceHandler},
    client::{MatrixAuth, MatrixClient},
    models::CreateRoom,
};
use serde_json::{json, Value};
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
        let auth = MatrixAuth::new(&config.bridge.appservice_token);
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
            &config.bridge.homeserver_token,
            &config.bridge.appservice_token,
            client,
        )
        .with_appservice_id(&config.bridge.bridge_id)
        .with_handler(Arc::new(HandlerWrapper(handler.clone())));

        Ok(Self {
            config,
            appservice,
            handler,
        })
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

        let mut opt = CreateRoom::default();
        opt.room_alias_name = Some(alias_localpart);
        opt.name = Some(name.to_owned());
        opt.topic = topic.map(ToOwned::to_owned);

        let room_id = self.appservice.client.create_room(&opt).await?;
        Ok(room_id)
    }

    pub async fn send_message(&self, room_id: &str, sender: &str, content: &str) -> Result<()> {
        self.send_message_with_metadata(room_id, sender, content, &[], None, None)
            .await
    }

    pub async fn send_notice(&self, room_id: &str, content: &str) -> Result<()> {
        let content_json = json!({
            "msgtype": "m.notice",
            "body": content
        });
        self.appservice
            .client
            .send_state_event(room_id, "m.room.message", "", &content_json)
            .await?;
        Ok(())
    }

    pub async fn send_message_with_metadata(
        &self,
        room_id: &str,
        sender: &str,
        body: &str,
        _attachments: &[String], // TODO: Implement attachment uploading
        reply_to: Option<&str>,
        _edit_of: Option<&str>,
    ) -> Result<()> {
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

        // Send state event is for state, we should send a normal message event...
        // Wait, matrix_bot_sdk doesn't have send_room_message directly yet?
        // Let's use `raw_json` for /send/m.room.message
        let txn_id = uuid::Uuid::new_v4().to_string();
        ghost_client
            .raw_json(
                reqwest::Method::PUT,
                &format!(
                    "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
                    percent_encoding::utf8_percent_encode(
                        room_id,
                        percent_encoding::NON_ALPHANUMERIC
                    ),
                    txn_id
                ),
                Some(content),
            )
            .await?;

        Ok(())
    }

    pub async fn check_permission(
        &self,
        _user_id: &str,
        _room_id: &str,
        _required_level: i64,
        _category: &str,
        _subcategory: &str,
    ) -> Result<bool> {
        Ok(true) // TODO: Check power levels from Matrix state
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

    pub async fn set_room_alias(&self, _room_id: &str, alias: &str) -> Result<()> {
        let content = json!({ "alias": alias });
        self.appservice
            .client
            .raw_json(
                reqwest::Method::PUT,
                &format!(
                    "/_matrix/client/v3/directory/room/{}",
                    percent_encoding::utf8_percent_encode(
                        alias,
                        percent_encoding::NON_ALPHANUMERIC
                    )
                ),
                Some(content),
            )
            .await?;
        Ok(())
    }

    pub fn registration_preview(&self) -> Value {
        json!({
            "id": self.config.bridge.bridge_id,
            "url": format!("http://{}:{}", self.config.bridge.bind_address, self.config.bridge.port),
            "as_token": self.config.bridge.appservice_token,
            "hs_token": self.config.bridge.homeserver_token,
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
