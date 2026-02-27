use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use serenity::all::{
    Client as SerenityClient, Context as SerenityContext, EventHandler as SerenityEventHandler,
    ChannelId, GatewayIntents, GuildId, Http, Message as SerenityMessage, MessageId,
    MessageUpdateEvent, OnlineStatus, PermissionOverwrite, Permissions,
    PermissionOverwriteType, Presence, Ready, TypingStartEvent, UserId, Webhook, WebhookType,
    ExecuteWebhook, CreateAttachment, CreateMessage,
};
use tokio::sync::{RwLock, oneshot, Mutex as AsyncMutex};

use crate::bridge::presence_handler::{DiscordActivity, DiscordPresence, DiscordPresenceState};
use crate::bridge::{BridgeCore, DiscordMessageContext};
use crate::config::Config;

const INITIAL_LOGIN_RETRY_SECONDS: u64 = 2;
const MAX_LOGIN_RETRY_SECONDS: u64 = 300;

pub mod command_handler;
pub mod embed;

pub use self::command_handler::{DiscordCommandHandler, DiscordCommandOutcome, ModerationAction};
pub use self::embed::{DiscordEmbed, EmbedAuthor, EmbedFooter, build_matrix_message_embed, build_reply_embed};

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
    http: Arc<RwLock<Option<Arc<Http>>>>,
    webhook_cache: Arc<RwLock<std::collections::HashMap<String, WebhookInfo>>>,
    our_webhook_ids: Arc<RwLock<std::collections::HashSet<u64>>>,
}

struct DiscordLoginState {
    is_logged_in: bool,
    gateway_task: Option<tokio::task::JoinHandle<()>>,
}

impl Default for DiscordLoginState {
    fn default() -> Self {
        Self {
            is_logged_in: false,
            gateway_task: None,
        }
    }
}

#[derive(Clone)]
struct WebhookInfo {
    id: u64,
    token: String,
}

struct ReadySignalHandler {
    ready_sender: Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,
    bridge: Arc<RwLock<Option<Arc<BridgeCore>>>>,
    http_sender: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Arc<Http>>>>>,
    our_webhook_ids: Arc<RwLock<std::collections::HashSet<u64>>>,
}

#[serenity::async_trait]
impl SerenityEventHandler for ReadySignalHandler {
    async fn ready(&self, ctx: SerenityContext, ready: Ready) {
        info!(
            "discord gateway ready as {} ({})",
            ready.user.name, ready.user.id
        );
        if let Some(sender) = self.ready_sender.lock().await.take() {
            let _ = sender.send(());
        }
        if let Some(sender) = self.http_sender.lock().await.take() {
            let _ = sender.send(ctx.http);
        }
    }

    async fn message(&self, _ctx: SerenityContext, msg: SerenityMessage) {
        if msg.author.bot {
            return;
        }

        if let Some(webhook_id) = msg.webhook_id {
            let our_ids = self.our_webhook_ids.read().await;
            if our_ids.contains(&webhook_id.get()) {
                debug!(
                    "ignoring discord message from our own webhook webhook_id={} message_id={}",
                    webhook_id, msg.id
                );
                return;
            }
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

        for message_id in unique_message_ids(deleted_messages_ids) {
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

    async fn user_update(&self, _ctx: SerenityContext, _old: Option<serenity::model::user::CurrentUser>, new: serenity::model::user::CurrentUser) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if new.bot {
            return;
        }

        if let Err(err) = bridge
            .handle_discord_user_update(&new.id.to_string(), &new.name, new.avatar_url().as_deref())
            .await
        {
            error!("failed to handle discord user update: {err}");
        }
    }

    async fn guild_member_update(&self, _ctx: SerenityContext, _old: Option<serenity::model::guild::Member>, new: Option<serenity::model::guild::Member>, _event: serenity::model::event::GuildMemberUpdateEvent) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        let Some(new) = new else {
            return;
        };

        if new.user.bot {
            return;
        }

        let display_name = new.nick.as_ref().unwrap_or(&new.user.name);
        let avatar_url = new.avatar_url().or_else(|| new.user.avatar_url());
        let roles: Vec<String> = new.roles.iter().map(|role_id| role_id.to_string()).collect();

        if let Err(err) = bridge
            .handle_discord_guild_member_update(
                &new.guild_id.to_string(),
                &new.user.id.to_string(),
                display_name,
                avatar_url.as_deref(),
                &roles,
            )
            .await
        {
            error!("failed to handle discord guild member update: {err}");
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

    async fn channel_update(&self, _ctx: SerenityContext, _old: Option<serenity::model::channel::GuildChannel>, new: serenity::model::channel::GuildChannel) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_channel_update(&new.id.to_string(), &new.name, new.topic.as_deref())
            .await
        {
            error!("failed to handle discord channel update: {err}");
        }
    }

    async fn channel_delete(&self, _ctx: SerenityContext, channel: serenity::model::channel::GuildChannel, _messages: Option<Vec<SerenityMessage>>) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_channel_delete(&channel.id.to_string())
            .await
        {
            error!("failed to handle discord channel delete: {err}");
        }
    }

    async fn guild_update(&self, _ctx: SerenityContext, _old: Option<serenity::model::guild::Guild>, new: serenity::model::guild::PartialGuild) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_guild_update(&new.id.to_string(), &new.name, new.icon_url().as_deref())
            .await
        {
            error!("failed to handle discord guild update: {err}");
        }
    }

    async fn guild_delete(&self, _ctx: SerenityContext, incomplete: serenity::model::guild::UnavailableGuild, _full: Option<serenity::model::guild::Guild>) {
        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_guild_delete(&incomplete.id.to_string())
            .await
        {
            error!("failed to handle discord guild delete: {err}");
        }
    }

    async fn guild_member_addition(&self, _ctx: SerenityContext, member: serenity::model::guild::Member) {
        if member.user.bot {
            return;
        }

        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        let avatar_url = member.avatar_url().or_else(|| member.user.avatar_url());
        let avatar_url = avatar_url.as_deref();
        let roles: Vec<String> = member
            .roles
            .iter()
            .map(|role_id| role_id.to_string())
            .collect();

        if let Err(err) = bridge
            .handle_discord_guild_member_add(
                &member.guild_id.to_string(),
                &member.user.id.to_string(),
                member.nick.as_deref().unwrap_or(&member.user.name),
                avatar_url,
                &roles,
            )
            .await
        {
            error!("failed to handle discord guild member addition: {err}");
        }
    }

    async fn guild_member_removal(&self, _ctx: SerenityContext, guild_id: GuildId, user: serenity::model::user::User, _member: Option<serenity::model::guild::Member>) {
        if user.bot {
            return;
        }

        let bridge = self.bridge.read().await.clone();
        let Some(bridge) = bridge else {
            return;
        };

        if let Err(err) = bridge
            .handle_discord_guild_member_remove(
                &guild_id.to_string(),
                &user.id.to_string(),
            )
            .await
        {
            error!("failed to handle discord guild member removal: {err}");
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

fn unique_message_ids(ids: Vec<MessageId>) -> Vec<MessageId> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(*id))
        .collect()
}

impl DiscordClient {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!("initializing discord client");
        Ok(Self {
            _config: config,
            send_lock: Arc::new(tokio::sync::Mutex::new(())),
            login_state: Arc::new(tokio::sync::Mutex::new(DiscordLoginState::default())),
            bridge: Arc::new(RwLock::new(None)),
            http: Arc::new(RwLock::new(None)),
            webhook_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
            our_webhook_ids: Arc::new(RwLock::new(std::collections::HashSet::new())),
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
        let (http_tx, http_rx) = oneshot::channel();
        let event_handler = ReadySignalHandler {
            ready_sender: Arc::new(tokio::sync::Mutex::new(Some(ready_tx))),
            bridge: self.bridge.clone(),
            http_sender: Arc::new(tokio::sync::Mutex::new(Some(http_tx))),
            our_webhook_ids: self.our_webhook_ids.clone(),
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
                
                if let Ok(http) = tokio::time::timeout(std::time::Duration::from_secs(5), http_rx).await {
                    if let Ok(http) = http {
                        *self.http.write().await = Some(http);
                    }
                }
                
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
        self.send_message_with_metadata_as_user(
            channel_id,
            content,
            attachments,
            reply_to,
            edit_of,
            None,
            None,
        ).await
    }

    pub async fn send_message_with_metadata_as_user(
        &self,
        channel_id: &str,
        content: &str,
        attachments: &[String],
        reply_to: Option<&str>,
        edit_of: Option<&str>,
        username: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<String> {
        debug!(
            "Discord send channel={} reply_to={:?} edit_of={:?} attachments={} username={:?} content={}",
            channel_id,
            reply_to,
            edit_of,
            attachments.len(),
            username,
            content
        );

        let _guard = self.send_lock.lock().await;

        let delay = self._config.limits.discord_send_delay;
        if delay > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        let http_guard = self.http.read().await;
        let Some(http) = http_guard.as_ref() else {
            warn!("discord http client not available, using mock");
            return self.send_message(channel_id, content).await;
        };

        let channel_id_num: u64 = channel_id.parse()
            .map_err(|_| anyhow!("invalid channel id: {}", channel_id))?;

        if self._config.channel.enable_webhook && username.is_some() {
            match self.get_or_create_webhook(http, channel_id_num).await {
                Ok(webhook_info) => {
                    return self.send_via_webhook(
                        http,
                        &webhook_info,
                        content,
                        attachments,
                        reply_to,
                        edit_of,
                        username.unwrap(),
                        avatar_url,
                    ).await;
                }
                Err(err) => {
                    warn!("failed to get/create webhook, falling back to direct send: {}", err);
                }
            }
        }

        self.send_direct_message(http, channel_id_num, content, attachments, reply_to, edit_of).await
    }

    pub async fn send_embed_as_user(
        &self,
        channel_id: &str,
        embed: &DiscordEmbed,
        username: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<String> {
        debug!(
            "Discord send embed channel={} username={:?}",
            channel_id,
            username
        );

        let _guard = self.send_lock.lock().await;

        let delay = self._config.limits.discord_send_delay;
        if delay > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        let http_guard = self.http.read().await;
        let Some(http) = http_guard.as_ref() else {
            warn!("discord http client not available");
            return Err(anyhow!("discord http client not available"));
        };

        let channel_id_num: u64 = channel_id.parse()
            .map_err(|_| anyhow!("invalid channel id: {}", channel_id))?;

        if self._config.channel.enable_webhook && username.is_some() {
            match self.get_or_create_webhook(http, channel_id_num).await {
                Ok(webhook_info) => {
                    return self.send_embed_via_webhook(
                        http,
                        &webhook_info,
                        embed,
                        username.unwrap(),
                        avatar_url,
                    ).await;
                }
                Err(err) => {
                    warn!("failed to get/create webhook for embed, falling back: {}", err);
                }
            }
        }

        let channel = ChannelId::new(channel_id_num);
        use serenity::builder::{CreateMessage, CreateEmbed};
        
        let mut embed_builder = CreateEmbed::new();
        
        if let Some(ref description) = embed.description {
            embed_builder = embed_builder.description(description);
        }
        
        if let Some(ref title) = embed.title {
            embed_builder = embed_builder.title(title);
        }
        
        if let Some(color) = embed.color {
            embed_builder = embed_builder.color(color);
        }
        
        if let Some(author) = &embed.author {
            embed_builder = embed_builder.author(
                serenity::builder::CreateEmbedAuthor::new(&author.name)
                    .icon_url(author.icon_url.as_deref().unwrap_or(""))
            );
        }
        
        for field in &embed.fields {
            embed_builder = embed_builder.field(&field.name, &field.value, field.inline);
        }

        let message = channel
            .send_message(http, CreateMessage::new().embed(embed_builder))
            .await
            .map_err(|e| anyhow!("failed to send embed to discord: {}", e))?;

        info!("sent embed to channel {}, message_id={}", channel_id, message.id);
        Ok(message.id.to_string())
    }

    async fn send_embed_via_webhook(
        &self,
        http: &Http,
        webhook_info: &WebhookInfo,
        embed: &DiscordEmbed,
        username: &str,
        avatar_url: Option<&str>,
    ) -> Result<String> {
        let webhook_url = format!(
            "https://discord.com/api/webhooks/{}/{}",
            webhook_info.id, webhook_info.token
        );

        let webhook = Webhook::from_url(http, &webhook_url).await
            .map_err(|e| anyhow!("failed to parse webhook url: {}", e))?;

        use serenity::builder::CreateEmbed;

        let mut embed_builder = CreateEmbed::new();
        
        if let Some(ref description) = embed.description {
            embed_builder = embed_builder.description(description);
        }
        
        if let Some(ref title) = embed.title {
            embed_builder = embed_builder.title(title);
        }
        
        if let Some(color) = embed.color {
            embed_builder = embed_builder.color(color);
        }
        
        if let Some(author) = &embed.author {
            embed_builder = embed_builder.author(
                serenity::builder::CreateEmbedAuthor::new(&author.name)
                    .icon_url(author.icon_url.as_deref().unwrap_or(""))
            );
        }
        
        for field in &embed.fields {
            embed_builder = embed_builder.field(&field.name, &field.value, field.inline);
        }

        let mut builder = ExecuteWebhook::new()
            .username(username)
            .embeds(vec![embed_builder]);

        if let Some(avatar) = avatar_url {
            builder = builder.avatar_url(avatar);
        }

        let message = webhook.execute(http, false, builder).await
            .map_err(|e| anyhow!("webhook embed send failed: {}", e))?
            .ok_or_else(|| anyhow!("webhook execution returned no message"))?;

        info!("sent embed via webhook to channel, message_id={}", message.id);
        Ok(message.id.to_string())
    }

    async fn get_or_create_webhook(&self, http: &Http, channel_id: u64) -> Result<WebhookInfo> {
        if let Some(info) = self.webhook_cache.read().await.get(&channel_id.to_string()) {
            return Ok(info.clone());
        }

        let channel = ChannelId::new(channel_id);
        let webhooks = channel.webhooks(http).await
            .map_err(|e| anyhow!("failed to fetch webhooks: {}", e))?;

        let webhook_name = &self._config.channel.webhook_name;
        let existing = webhooks.iter().find(|w| w.name.as_deref() == Some(webhook_name));

        let info = if let Some(webhook) = existing {
            let token = webhook.token.clone()
                .ok_or_else(|| anyhow!("webhook has no token"))?
                .expose_secret()
                .to_string();
            WebhookInfo {
                id: webhook.id.get(),
                token,
            }
        } else {
            use serenity::builder::CreateWebhook;
            let webhook: serenity::model::webhook::Webhook = channel
                .create_webhook(http, CreateWebhook::new(webhook_name))
                .await
                .map_err(|e| anyhow!("failed to create webhook: {}", e))?;
            
            let token = webhook.token.clone()
                .ok_or_else(|| anyhow!("created webhook has no token"))?
                .expose_secret()
                .to_string();
            
            WebhookInfo {
                id: webhook.id.get(),
                token,
            }
        };

        self.our_webhook_ids.write().await.insert(info.id);
        debug!("recorded our webhook id={} for channel={}", info.id, channel_id);
        
        self.webhook_cache.write().await.insert(channel_id.to_string(), info.clone());
        Ok(info)
    }

    async fn send_via_webhook(
        &self,
        http: &Http,
        webhook_info: &WebhookInfo,
        content: &str,
        _attachments: &[String],
        _reply_to: Option<&str>,
        edit_of: Option<&str>,
        username: &str,
        avatar_url: Option<&str>,
    ) -> Result<String> {
        use serenity::builder::{ExecuteWebhook, EditWebhookMessage};
        
        let webhook_url = format!(
            "https://discord.com/api/webhooks/{}/{}",
            webhook_info.id, webhook_info.token
        );
        
        let webhook = Webhook::from_url(http, &webhook_url).await
            .map_err(|e| anyhow!("failed to parse webhook url: {}", e))?;

        if let Some(message_id_str) = edit_of {
            let message_id: u64 = message_id_str.parse()
                .map_err(|e| anyhow!("invalid message id for edit: {}", e))?;
            
            let builder = EditWebhookMessage::new()
                .content(content);
            
            webhook.edit_message(http, MessageId::new(message_id), builder).await
                .map_err(|e| anyhow!("webhook edit failed: {}", e))?;
            
            info!("edited message via webhook, message_id={}", message_id_str);
            return Ok(message_id_str.to_string());
        }

        let mut builder = ExecuteWebhook::new()
            .content(content)
            .username(username);
        
        if let Some(avatar) = avatar_url {
            builder = builder.avatar_url(avatar);
        }

        let message = webhook.execute(http, false, builder).await
            .map_err(|e| anyhow!("webhook send failed: {}", e))?
            .ok_or_else(|| anyhow!("webhook execution returned no message"))?;

        info!("sent message via webhook to channel, message_id={}", message.id);
        Ok(message.id.to_string())
    }

    async fn send_direct_message(
        &self,
        http: &Http,
        channel_id: u64,
        content: &str,
        attachments: &[String],
        _reply_to: Option<&str>,
        edit_of: Option<&str>,
    ) -> Result<String> {
        use serenity::builder::{CreateMessage, EditMessage};
        
        let channel = ChannelId::new(channel_id);
        
        let mut message_content = content.to_string();
        for attachment in attachments {
            if !message_content.is_empty() {
                message_content.push('\n');
            }
            message_content.push_str(attachment);
        }

        if let Some(message_id_str) = edit_of {
            let message_id: u64 = message_id_str.parse()
                .map_err(|e| anyhow!("invalid message id for edit: {}", e))?;
            
            let message = channel.edit_message(http, MessageId::new(message_id), EditMessage::new().content(&message_content)).await
                .map_err(|e| anyhow!("direct message edit failed: {}", e))?;
            
            info!("edited message directly in channel {}, message_id={}", channel_id, message.id);
            return Ok(message.id.to_string());
        }

        let message = channel.send_message(http, CreateMessage::new().content(&message_content)).await
            .map_err(|e| anyhow!("direct message send failed: {}", e))?;

        info!("sent message directly to channel {}, message_id={}", channel_id, message.id);
        Ok(message.id.to_string())
    }

    pub async fn send_file_as_user(
        &self,
        channel_id: &str,
        data: &[u8],
        _content_type: &str,
        filename: &str,
        username: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<String> {
        debug!(
            "Discord send file channel={} filename={} size={} username={:?}",
            channel_id,
            filename,
            data.len(),
            username
        );

        let _guard = self.send_lock.lock().await;

        let delay = self._config.limits.discord_send_delay;
        if delay > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        let http_guard = self.http.read().await;
        let Some(http) = http_guard.as_ref() else {
            warn!("discord http client not available");
            return Err(anyhow!("discord http client not available"));
        };

        let channel_id_num: u64 = channel_id.parse()
            .map_err(|_| anyhow!("invalid channel id: {}", channel_id))?;

        if self._config.channel.enable_webhook && username.is_some() {
            match self.get_or_create_webhook(http, channel_id_num).await {
                Ok(webhook_info) => {
                    return self.send_file_via_webhook(
                        http,
                        &webhook_info,
                        data,
                        filename,
                        username.unwrap(),
                        avatar_url,
                    ).await;
                }
                Err(err) => {
                    warn!("failed to get/create webhook for file, falling back: {}", err);
                }
            }
        }

        let channel = ChannelId::new(channel_id_num);
        let attachment = CreateAttachment::bytes(data.to_vec(), filename);

        let message = channel
            .send_message(http, CreateMessage::new().add_file(attachment))
            .await
            .map_err(|e| anyhow!("failed to send file to discord: {}", e))?;

        info!("sent file to channel {}, message_id={}", channel_id, message.id);
        Ok(message.id.to_string())
    }

    async fn send_file_via_webhook(
        &self,
        http: &Http,
        webhook_info: &WebhookInfo,
        data: &[u8],
        filename: &str,
        username: &str,
        avatar_url: Option<&str>,
    ) -> Result<String> {
        let webhook_url = format!(
            "https://discord.com/api/webhooks/{}/{}",
            webhook_info.id, webhook_info.token
        );

        let webhook = Webhook::from_url(http, &webhook_url).await
            .map_err(|e| anyhow!("failed to parse webhook url: {}", e))?;

        let attachment = CreateAttachment::bytes(data.to_vec(), filename);

        let mut builder = ExecuteWebhook::new()
            .username(username);

        if let Some(avatar) = avatar_url {
            builder = builder.avatar_url(avatar);
        }

        builder = builder.add_file(attachment);

        let message = webhook.execute(http, false, builder).await
            .map_err(|e| anyhow!("webhook file send failed: {}", e))?
            .ok_or_else(|| anyhow!("webhook execution returned no message"))?;

        info!("sent file via webhook to channel, message_id={}", message.id);
        Ok(message.id.to_string())
    }

    pub async fn get_user(&self, user_id: &str) -> Result<Option<DiscordUser>> {
        Ok(Some(DiscordUser {
            id: user_id.to_string(),
            username: format!("user_{}", user_id),
            discriminator: "0001".to_string(),
            avatar: None,
        }))
    }

    pub async fn clear_channel_member_overwrite(
        &self,
        channel_id: &str,
        user_id: &str,
    ) -> Result<()> {
        let channel_id_num: u64 = channel_id
            .parse()
            .map_err(|_| anyhow!("invalid channel id: {}", channel_id))?;
        let user_id_num: u64 = user_id
            .parse()
            .map_err(|_| anyhow!("invalid user id: {}", user_id))?;

        let http_guard = self.http.read().await;
        let Some(http) = http_guard.as_ref() else {
            return Err(anyhow!("discord http client not available"));
        };

        ChannelId::new(channel_id_num)
            .delete_permission(
                http,
                PermissionOverwriteType::Member(UserId::new(user_id_num)),
            )
            .await
            .map_err(|e| anyhow!("failed to clear channel overwrite: {}", e))?;

        Ok(())
    }

    pub async fn deny_channel_member_permissions(
        &self,
        channel_id: &str,
        user_id: &str,
    ) -> Result<()> {
        let channel_id_num: u64 = channel_id
            .parse()
            .map_err(|_| anyhow!("invalid channel id: {}", channel_id))?;
        let user_id_num: u64 = user_id
            .parse()
            .map_err(|_| anyhow!("invalid user id: {}", user_id))?;

        let http_guard = self.http.read().await;
        let Some(http) = http_guard.as_ref() else {
            return Err(anyhow!("discord http client not available"));
        };

        let overwrite = PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::SEND_MESSAGES | Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Member(UserId::new(user_id_num)),
        };

        ChannelId::new(channel_id_num)
            .create_permission(http, overwrite)
            .await
            .map_err(|e| anyhow!("failed to set channel overwrite: {}", e))?;

        Ok(())
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
    use super::{permissions_to_names, unique_message_ids};
    use serenity::all::{MessageId, Permissions};

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

    #[test]
    fn unique_message_ids_deduplicates_and_preserves_order() {
        let ids = vec![
            MessageId::new(42),
            MessageId::new(99),
            MessageId::new(42),
            MessageId::new(7),
            MessageId::new(99),
        ];

        let deduped = unique_message_ids(ids);

        assert_eq!(deduped, vec![MessageId::new(42), MessageId::new(99), MessageId::new(7)]);
    }
}
