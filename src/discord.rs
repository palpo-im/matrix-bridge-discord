use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use serenity::all::{
    Client as SerenityClient, Context as SerenityContext, EventHandler as SerenityEventHandler,
    ChannelId, GatewayIntents, GuildId, Message as SerenityMessage, MessageId,
    MessageUpdateEvent, OnlineStatus, Permissions, Presence, Ready, TypingStartEvent,
};
use tokio::sync::{RwLock, oneshot};

use crate::bridge::presence_handler::{DiscordActivity, DiscordPresence, DiscordPresenceState};
use crate::bridge::{BridgeCore, DiscordMessageContext};
use crate::config::Config;

const INITIAL_LOGIN_RETRY_SECONDS: u64 = 2;
const MAX_LOGIN_RETRY_SECONDS: u64 = 300;

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
    bridge: Arc<RwLock<Option<Arc<BridgeCore>>>>,
}

#[derive(Default)]
struct DiscordLoginState {
    is_logged_in: bool,
    gateway_task: Option<tokio::task::JoinHandle<()>>,
}

struct ReadySignalHandler {
    ready_sender: Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,
    bridge: Arc<RwLock<Option<Arc<BridgeCore>>>>,
}

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

    async fn message(&self, _ctx: SerenityContext, msg: SerenityMessage) {
        if msg.author.bot {
            return;
        }

        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            debug!("ignoring discord message before bridge binding");
            return;
        };

        let reply_to = msg.referenced_message.as_ref().map(|m| m.id.to_string());
        let attachments = msg.attachments.iter().map(|a| a.url.clone()).collect();

        let permission_flags = msg
            .member
            .as_ref()
            .and_then(|member| member.permissions)
            .unwrap_or_else(Permissions::empty);

        let permissions = permissions_to_names(permission_flags);

        if let Err(err) = bridge
            .handle_discord_message_with_context(DiscordMessageContext {
                channel_id: msg.channel_id.to_string(),
                source_message_id: Some(msg.id.to_string()),
                sender_id: msg.author.id.to_string(),
                content: msg.content.clone(),
                attachments,
                reply_to,
                edit_of: None,
                permissions,
            })
            .await
        {
            error!("failed to handle discord message: {err}");
        }
    }

    async fn presence_update(&self, _ctx: SerenityContext, new_data: Presence) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if new_data.user.bot.unwrap_or(false) {
            return;
        }

        let state = match new_data.status {
            OnlineStatus::Online => DiscordPresenceState::Online,
            OnlineStatus::DoNotDisturb => DiscordPresenceState::Dnd,
            OnlineStatus::Idle => DiscordPresenceState::Idle,
            OnlineStatus::Offline | OnlineStatus::Invisible => DiscordPresenceState::Offline,
            _ => DiscordPresenceState::Offline,
        };

        let activities = new_data
            .activities
            .iter()
            .map(|activity| DiscordActivity {
                kind: format!("{:?}", activity.kind),
                name: activity.name.clone(),
                url: activity.url.as_ref().map(ToString::to_string),
            })
            .collect();

        bridge.enqueue_discord_presence(DiscordPresence {
            user_id: new_data.user.id.to_string(),
            username: new_data.user.name.clone(),
            state,
            activities,
        });
    }

    async fn message_update(
        &self,
        _ctx: SerenityContext,
        _old_if_available: Option<SerenityMessage>,
        _new_if_available: Option<SerenityMessage>,
        update: MessageUpdateEvent,
    ) {
        if update.author.as_ref().is_some_and(|author| author.bot) {
            return;
        }

        let Some(content) = update.content.clone() else {
            return;
        };

        let sender_id = update
            .author
            .as_ref()
            .map(|author| author.id.to_string())
            .unwrap_or_default();
        if sender_id.is_empty() {
            return;
        }

        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_message_with_context(DiscordMessageContext {
                channel_id: update.channel_id.to_string(),
                source_message_id: Some(update.id.to_string()),
                sender_id,
                content,
                attachments: Vec::new(),
                reply_to: None,
                edit_of: Some(update.id.to_string()),
                permissions: std::collections::HashSet::new(),
            })
            .await
        {
            error!("failed to handle discord message update: {err}");
        }
    }

    async fn message_delete(
        &self,
        _ctx: SerenityContext,
        channel_id: ChannelId,
        deleted_message_id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_message_delete(
                &channel_id.to_string(),
                &deleted_message_id.to_string(),
            )
            .await
        {
            error!("failed to handle discord message delete: {err}");
        }
    }

    async fn message_delete_bulk(
        &self,
        _ctx: SerenityContext,
        channel_id: ChannelId,
        deleted_messages_ids: Vec<MessageId>,
        _guild_id: Option<GuildId>,
    ) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        for message_id in deleted_messages_ids {
            if let Err(err) = bridge
                .handle_discord_message_delete(&channel_id.to_string(), &message_id.to_string())
                .await
            {
                error!(
                    "failed to handle discord bulk message delete for {}: {err}",
                    message_id
                );
            }
        }
    }

    async fn typing_start(&self, _ctx: SerenityContext, event: TypingStartEvent) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_typing(&event.channel_id.to_string(), &event.user_id.to_string())
            .await
        {
            error!("failed to handle discord typing event: {err}");
        }
    }
}

fn permissions_to_names(perms: Permissions) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    if perms.contains(Permissions::MANAGE_WEBHOOKS) {
        names.insert("MANAGE_WEBHOOKS".to_string());
    }
    if perms.contains(Permissions::MANAGE_CHANNELS) {
        names.insert("MANAGE_CHANNELS".to_string());
    }
    if perms.contains(Permissions::BAN_MEMBERS) {
        names.insert("BAN_MEMBERS".to_string());
    }
    if perms.contains(Permissions::KICK_MEMBERS) {
        names.insert("KICK_MEMBERS".to_string());
    }
    names
}

impl DiscordClient {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!("initializing discord client");
        Ok(Self {
            _config: config,
            send_lock: Arc::new(tokio::sync::Mutex::new(())),
            login_state: Arc::new(tokio::sync::Mutex::new(DiscordLoginState::default())),
            bridge: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn set_bridge(&self, bridge: Arc<BridgeCore>) {
        *self.bridge.write().await = Some(bridge);
    }

    pub async fn login(&self) -> Result<()> {
        let mut state = self.login_state.lock().await;
        if state.is_logged_in {
            return Ok(());
        }

        let intents = if self._config.auth.use_privileged_intents {
            GatewayIntents::all()
        } else {
            GatewayIntents::non_privileged()
        };

        let (ready_tx, ready_rx) = oneshot::channel();
        let event_handler = ReadySignalHandler {
            ready_sender: Arc::new(tokio::sync::Mutex::new(Some(ready_tx))),
            bridge: self.bridge.clone(),
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

    pub async fn start(&self) -> Result<()> {
        let mut retry_seconds = INITIAL_LOGIN_RETRY_SECONDS;

        loop {
            match self.login().await {
                Ok(()) => {
                    info!("discord client is ready");
                    return Ok(());
                }
                Err(err) => {
                    error!(
                        "failed to start discord client: {err}. retrying in {} seconds",
                        retry_seconds
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(retry_seconds)).await;
                    retry_seconds = (retry_seconds * 2).min(MAX_LOGIN_RETRY_SECONDS);
                }
            }
        }
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.login_state.lock().await;
        if !state.is_logged_in {
            return Ok(());
        }

        if let Some(gateway_task) = state.gateway_task.take() {
            gateway_task.abort();
            match gateway_task.await {
                Ok(()) => info!("discord gateway task exited"),
                Err(join_err) if join_err.is_cancelled() => {
                    info!("discord gateway task aborted")
                }
                Err(join_err) => {
                    error!("discord gateway task join error: {join_err}");
                }
            }
        }

        state.is_logged_in = false;
        info!("discord client stopped");
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

#[cfg(test)]
mod tests {
    use super::permissions_to_names;
    use serenity::all::Permissions;

    #[test]
    fn permissions_to_names_maps_expected_flags() {
        let perms = Permissions::MANAGE_WEBHOOKS
            | Permissions::MANAGE_CHANNELS
            | Permissions::BAN_MEMBERS
            | Permissions::KICK_MEMBERS;

        let names = permissions_to_names(perms);

        assert!(names.contains("MANAGE_WEBHOOKS"));
        assert!(names.contains("MANAGE_CHANNELS"));
        assert!(names.contains("BAN_MEMBERS"));
        assert!(names.contains("KICK_MEMBERS"));
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn permissions_to_names_ignores_unmapped_flags() {
        let names = permissions_to_names(Permissions::SEND_MESSAGES);
        assert!(names.is_empty());
    }
}
