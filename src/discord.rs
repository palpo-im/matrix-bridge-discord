use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

#[cfg(feature = "discord")]
use serenity::all::{
    Client as SerenityClient, Context as SerenityContext, EventHandler as SerenityEventHandler,
    GatewayIntents, Ready,
};
#[cfg(feature = "discord")]
use tokio::sync::oneshot;

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
    send_lock: Arc<tokio::sync::Mutex<()>>,
    login_state: Arc<tokio::sync::Mutex<DiscordLoginState>>,
}

#[derive(Default)]
struct DiscordLoginState {
    is_logged_in: bool,
    #[cfg(feature = "discord")]
    gateway_task: Option<tokio::task::JoinHandle<()>>,
}

#[cfg(feature = "discord")]
struct ReadySignalHandler {
    ready_sender: Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,
}

#[cfg(feature = "discord")]
#[serenity::async_trait]
impl SerenityEventHandler for ReadySignalHandler {
    async fn ready(&self, _ctx: SerenityContext, ready: Ready) {
        info!(
            "discord gateway ready as {} ({})",
            ready.user.name, ready.user.id
        );
        if let Some(sender) = self.ready_sender.lock().await.take() {
            let _ = sender.send(());
        }
    }
}

impl DiscordClient {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!("initializing discord client");
        Ok(Self {
            _config: config,
            send_lock: Arc::new(tokio::sync::Mutex::new(())),
            login_state: Arc::new(tokio::sync::Mutex::new(DiscordLoginState::default())),
        })
    }

    pub async fn login(&self) -> Result<()> {
        let mut state = self.login_state.lock().await;
        if state.is_logged_in {
            return Ok(());
        }

        #[cfg(not(feature = "discord"))]
        {
            return Err(anyhow!(
                "Discord support is disabled at compile time (enable the `discord` feature)"
            ));
        }

        #[cfg(feature = "discord")]
        {
            let intents = if self._config.auth.use_privileged_intents {
                GatewayIntents::all()
            } else {
                GatewayIntents::non_privileged()
            };

            let (ready_tx, ready_rx) = oneshot::channel();
            let event_handler = ReadySignalHandler {
                ready_sender: Arc::new(tokio::sync::Mutex::new(Some(ready_tx))),
            };

            let mut gateway_client = SerenityClient::builder(&self._config.auth.bot_token, intents)
                .event_handler(event_handler)
                .await
                .map_err(|err| anyhow!("failed to build discord gateway client: {err}"))?;

            let gateway_task = tokio::spawn(async move {
                if let Err(err) = gateway_client.start_autosharded().await {
                    error!("discord gateway stopped: {err}");
                }
            });

            match tokio::time::timeout(std::time::Duration::from_secs(30), ready_rx).await {
                Ok(Ok(())) => {
                    state.is_logged_in = true;
                    state.gateway_task = Some(gateway_task);
                    info!("discord bot login succeeded and gateway is connected");
                    Ok(())
                }
                Ok(Err(_)) => {
                    gateway_task.abort();
                    Err(anyhow!(
                        "discord gateway exited before receiving Ready event"
                    ))
                }
                Err(_) => {
                    gateway_task.abort();
                    Err(anyhow!("timed out waiting for discord Ready event"))
                }
            }
        }
    }

    pub async fn start(&self) -> Result<()> {
        self.login().await?;
        info!("discord client is ready");
        Ok(())
    }

    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<String> {
        let _guard = self.send_lock.lock().await;

        let delay = self._config.limits.discord_send_delay;
        if delay > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        info!("forwarding message to discord channel {}", channel_id);
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
