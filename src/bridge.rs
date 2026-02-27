use std::sync::Arc;
use std::{collections::HashSet, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use tracing::{debug, info, warn};

use crate::db::{DatabaseManager, MessageMapping, RoomMapping};
use crate::discord::{
    DiscordClient, DiscordCommandHandler, DiscordCommandOutcome, ModerationAction,
};
use crate::matrix::{MatrixAppservice, MatrixCommandHandler, MatrixCommandOutcome, MatrixEvent};

pub mod message_flow;
pub mod presence_handler;
pub mod provisioning;

use self::message_flow::{
    DiscordInboundMessage, MessageFlow, OutboundDiscordMessage, OutboundMatrixMessage,
};
use self::presence_handler::{
    DiscordPresence, MatrixPresenceState, MatrixPresenceTarget, PresenceHandler,
};
use self::provisioning::{ApprovalResponseStatus, ProvisioningCoordinator, ProvisioningError};

#[derive(Debug, Clone)]
pub struct DiscordMessageContext {
    pub channel_id: String,
    pub source_message_id: Option<String>,
    pub sender_id: String,
    pub content: String,
    pub attachments: Vec<String>,
    pub reply_to: Option<String>,
    pub edit_of: Option<String>,
    pub permissions: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RedactionRequest {
    room_id: String,
    event_id: String,
    reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypingRequest {
    room_id: String,
    discord_user_id: String,
    typing: bool,
    timeout_ms: Option<u64>,
}

const DISCORD_TYPING_TIMEOUT_MS: u64 = 4000;

#[derive(Clone)]
pub struct BridgeCore {
    matrix_client: Arc<MatrixAppservice>,
    discord_client: Arc<DiscordClient>,
    db_manager: Arc<DatabaseManager>,
    message_flow: Arc<MessageFlow>,
    matrix_command_handler: Arc<MatrixCommandHandler>,
    discord_command_handler: Arc<DiscordCommandHandler>,
    presence_handler: Arc<PresenceHandler>,
    provisioning: Arc<ProvisioningCoordinator>,
}

impl BridgeCore {
    pub fn new(
        matrix_client: Arc<MatrixAppservice>,
        discord_client: Arc<DiscordClient>,
        db_manager: Arc<DatabaseManager>,
    ) -> Self {
        let bridge_config = matrix_client.config().bridge.clone();
        Self {
            message_flow: Arc::new(MessageFlow::new(
                matrix_client.clone(),
                discord_client.clone(),
            )),
            matrix_command_handler: Arc::new(MatrixCommandHandler::new(
                bridge_config.enable_self_service_bridging,
                None,
            )),
            discord_command_handler: Arc::new(DiscordCommandHandler::new()),
            presence_handler: Arc::new(PresenceHandler::new(None)),
            provisioning: Arc::new(ProvisioningCoordinator::default()),
            matrix_client,
            discord_client,
            db_manager,
        }
    }

    pub async fn start(&self) -> Result<()> {
        self.matrix_client.start().await?;
        self.discord_client.start().await?;

        info!("bridge core started");

        let bridge_config = self.matrix_client.config().bridge.clone();
        let presence_interval_ms = bridge_config.presence_interval.max(250);
        let mut ticker = tokio::time::interval(Duration::from_millis(presence_interval_ms));
        loop {
            ticker.tick().await;
            if !bridge_config.disable_presence {
                self.presence_handler
                    .process_next(self.matrix_client.as_ref())
                    .await?;
            }
            debug!("bridge heartbeat");
        }
    }

    pub async fn send_to_discord(
        &self,
        discord_channel_id: String,
        _matrix_sender: String,
        content: String,
    ) -> Result<()> {
        self.send_to_discord_message(
            &discord_channel_id,
            OutboundDiscordMessage {
                content,
                reply_to: None,
                edit_of: None,
                attachments: Vec::new(),
            },
        )
        .await
    }

    pub async fn send_to_matrix(
        &self,
        matrix_room_id: String,
        discord_sender: String,
        content: String,
    ) -> Result<()> {
        self.send_to_matrix_message(
            &matrix_room_id,
            &discord_sender,
            OutboundMatrixMessage {
                body: content,
                reply_to: None,
                edit_of: None,
                attachments: Vec::new(),
            },
        )
        .await
        .map(|_| ())
    }

    pub async fn handle_matrix_message(&self, event: &MatrixEvent) -> Result<()> {
        let body = event
            .content
            .as_ref()
            .map(crate::parsers::MessageUtils::extract_plain_text)
            .unwrap_or_default();

        debug!(
            "matrix inbound message event_id={:?} room_id={} sender={} type={} body_len={} body_preview={}",
            event.event_id,
            event.room_id,
            event.sender,
            event.event_type,
            body.len(),
            preview_text(&body)
        );

        let room_mapping = self
            .db_manager
            .room_store()
            .get_room_by_matrix_room(&event.room_id)
            .await?;

        debug!(
            "matrix inbound mapping lookup room_id={} mapped={}",
            event.room_id,
            room_mapping.is_some()
        );

        if self.matrix_command_handler.is_command(&body) {
            debug!(
                "matrix inbound command detected room_id={} sender={} command_preview={}",
                event.room_id,
                event.sender,
                preview_text(&body)
            );
            let has_permissions = self
                .matrix_client
                .check_permission(
                    &event.sender,
                    &event.room_id,
                    50,
                    "events",
                    "m.room.power_levels",
                )
                .await
                .unwrap_or(false);
            debug!(
                "matrix command permission result room_id={} sender={} granted={}",
                event.room_id, event.sender, has_permissions
            );
            let outcome = self
                .matrix_command_handler
                .handle(&body, room_mapping.is_some(), |_| Ok(has_permissions));
            self.handle_matrix_command_outcome(outcome, event).await?;
            return Ok(());
        }

        let Some(mapping) = room_mapping else {
            debug!(
                "matrix inbound dropped room_id={} reason=no_discord_mapping",
                event.room_id
            );
            return Ok(());
        };
        let Some(message) = MessageFlow::parse_matrix_event(event) else {
            debug!(
                "matrix inbound dropped room_id={} event_id={:?} reason=unsupported_or_unparseable",
                event.room_id, event.event_id
            );
            return Ok(());
        };

        let outbound = self.message_flow.matrix_to_discord(&message);
        debug!(
            "matrix->discord outbound prepared room_id={} discord_channel={} reply_to={:?} edit_of={:?} attachments={} content_len={} content_preview={}",
            mapping.matrix_room_id,
            mapping.discord_channel_id,
            outbound.reply_to,
            outbound.edit_of,
            outbound.attachments.len(),
            outbound.content.len(),
            preview_text(&outbound.content)
        );
        self.send_to_discord_message(&mapping.discord_channel_id, outbound)
            .await?;
        Ok(())
    }

    async fn handle_matrix_command_outcome(
        &self,
        outcome: MatrixCommandOutcome,
        event: &MatrixEvent,
    ) -> Result<()> {
        match outcome {
            MatrixCommandOutcome::Ignored => {}
            MatrixCommandOutcome::Reply(reply) => {
                self.matrix_client
                    .send_notice(&event.room_id, &reply)
                    .await?;
            }
            MatrixCommandOutcome::BridgeRequested {
                guild_id,
                channel_id,
            } => {
                let reply = self
                    .request_bridge_matrix_room(
                        &event.room_id,
                        &event.sender,
                        &guild_id,
                        &channel_id,
                    )
                    .await?;
                self.matrix_client
                    .send_notice(&event.room_id, &reply)
                    .await?;
            }
            MatrixCommandOutcome::UnbridgeRequested => {
                let reply = self.unbridge_matrix_room(&event.room_id).await?;
                self.matrix_client
                    .send_notice(&event.room_id, &reply)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn handle_matrix_member(&self, event: &MatrixEvent) -> Result<()> {
        if let Some(content) = event.content.as_ref().and_then(|c| c.as_object()) {
            if let Some(membership) = content.get("membership").and_then(|v| v.as_str()) {
                let bot_user_id = self.matrix_client.bot_user_id();
                if membership == "invite"
                    && event.state_key.as_deref() == Some(bot_user_id.as_str())
                {
                    match self
                        .matrix_client
                        .appservice
                        .client
                        .join_room(&event.room_id)
                        .await
                    {
                        Ok(joined) => {
                            info!("joined invited room {}", joined);
                        }
                        Err(err) => {
                            warn!("failed to join invited room {}: {}", event.room_id, err);
                        }
                    }
                }

                if membership == "leave"
                    && let Some(state_key) = &event.state_key
                    && event.sender != *state_key
                {
                    let kick_for = self.matrix_client.config().room.kick_for;
                    if kick_for > 0 {
                        let target_user = state_key.clone();
                        let room_id = event.room_id.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_millis(kick_for)).await;
                            info!(
                                "Lifting kick for {} in {} after {} ms, restoring Discord permissions",
                                target_user, room_id, kick_for
                            );
                            // TODO: Actually overwrite Discord permissions via DiscordClient when fully implemented
                        });
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn request_bridge_matrix_room(
        &self,
        matrix_room_id: &str,
        matrix_requestor: &str,
        guild_id: &str,
        channel_id: &str,
    ) -> Result<String> {
        if let Some(limit_message) = self.check_room_limit().await? {
            return Ok(limit_message);
        }

        if self
            .db_manager
            .room_store()
            .get_room_by_discord_channel(channel_id)
            .await?
            .is_some()
        {
            return Ok("This Discord channel is already bridged.".to_string());
        }

        let Some(channel) = self.discord_client.get_channel(channel_id).await? else {
            return Ok(
                "There was a problem bridging that channel - channel was not found.".to_string(),
            );
        };

        self.matrix_client
            .send_notice(
                matrix_room_id,
                "I'm asking permission from the guild administrators to make this bridge.",
            )
            .await?;

        match self
            .provisioning
            .ask_bridge_permission(self.discord_client.as_ref(), &channel.id, matrix_requestor)
            .await
        {
            Ok(()) => {
                self.bridge_matrix_room(matrix_room_id, guild_id, channel_id)
                    .await
            }
            Err(ProvisioningError::TimedOut) => {
                Ok("Timed out waiting for a response from the Discord owners.".to_string())
            }
            Err(ProvisioningError::Declined) => {
                Ok("The bridge has been declined by the Discord guild.".to_string())
            }
            Err(err) => {
                warn!(
                    "failed to obtain bridge approval for matrix_room={} channel={}: {}",
                    matrix_room_id, channel_id, err
                );
                Ok("There was a problem bridging that channel - has the guild owner approved the bridge?".to_string())
            }
        }
    }

    pub async fn bridge_matrix_room(
        &self,
        matrix_room_id: &str,
        guild_id: &str,
        channel_id: &str,
    ) -> Result<String> {
        if let Some(limit_message) = self.check_room_limit().await? {
            return Ok(limit_message);
        }

        if self
            .db_manager
            .room_store()
            .get_room_by_discord_channel(channel_id)
            .await?
            .is_some()
        {
            return Ok("This Discord channel is already bridged.".to_string());
        }

        let Some(channel) = self.discord_client.get_channel(channel_id).await? else {
            return Ok(
                "There was a problem bridging that channel - channel was not found.".to_string(),
            );
        };

        let mapping = RoomMapping {
            id: 0,
            matrix_room_id: matrix_room_id.to_string(),
            discord_channel_id: channel.id.clone(),
            discord_channel_name: channel.name.clone(),
            discord_guild_id: guild_id.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.db_manager
            .room_store()
            .create_room_mapping(&mapping)
            .await?;

        let name_pattern = &self.matrix_client.config().channel.name_pattern;
        let formatted_name = crate::utils::formatting::apply_pattern_string(
            name_pattern,
            &[
                ("guild", &channel.guild_id.clone()),
                ("name", &format!("#{}", mapping.discord_channel_name)),
            ],
        );

        let event_content = serde_json::json!({
            "name": formatted_name
        });
        let _ = self
            .matrix_client
            .appservice
            .client
            .send_state_event(matrix_room_id, "m.room.name", "", &event_content)
            .await;

        Ok("I have bridged this room to your channel".to_string())
    }

    async fn check_room_limit(&self) -> Result<Option<String>> {
        let room_count_limit = self.matrix_client.config().limits.room_count;
        if room_count_limit < 0 {
            return Ok(None);
        }

        let current_count = self.db_manager.room_store().count_rooms().await?;
        if current_count >= room_count_limit as i64 {
            Ok(Some(format!(
                "This bridge has reached its room limit of {}. Unbridge another room to allow for new connections.",
                room_count_limit
            )))
        } else {
            Ok(None)
        }
    }

    pub async fn unbridge_matrix_room(&self, matrix_room_id: &str) -> Result<String> {
        let room_mapping = self
            .db_manager
            .room_store()
            .get_room_by_matrix_room(matrix_room_id)
            .await?;

        let Some(mapping) = room_mapping else {
            return Ok("This room is not bridged.".to_string());
        };

        let delete_options = &self.matrix_client.config().channel.delete_options;
        let client = &self.matrix_client.appservice.client;

        if let Some(prefix) = &delete_options.name_prefix {
            if let Ok(state) = client
                .get_room_state_event(matrix_room_id, "m.room.name", "")
                .await
            {
                if let Some(name) = state.get("name").and_then(|n| n.as_str()) {
                    let new_name = format!("{}{}", prefix, name);
                    let event_content = serde_json::json!({ "name": new_name });
                    let _ = client
                        .send_state_event(matrix_room_id, "m.room.name", "", &event_content)
                        .await;
                }
            }
        }

        if let Some(prefix) = &delete_options.topic_prefix {
            if let Ok(state) = client
                .get_room_state_event(matrix_room_id, "m.room.topic", "")
                .await
            {
                if let Some(topic) = state.get("topic").and_then(|t| t.as_str()) {
                    let new_topic = format!("{}{}", prefix, topic);
                    let event_content = serde_json::json!({ "topic": new_topic });
                    let _ = client
                        .send_state_event(matrix_room_id, "m.room.topic", "", &event_content)
                        .await;
                }
            }
        }

        if delete_options.unset_room_alias {
            let alias_localpart = format!(
                "{}{}",
                self.matrix_client.config().room.room_alias_prefix,
                mapping.discord_channel_id
            );
            let alias = format!(
                "#{}:{}",
                alias_localpart,
                self.matrix_client.config().bridge.domain
            );
            let _ = client.delete_room_alias(&alias).await;
        }

        self.db_manager
            .room_store()
            .delete_room_mapping(mapping.id)
            .await?;
        Ok("This room has been unbridged".to_string())
    }

    pub async fn send_to_discord_message(
        &self,
        discord_channel_id: &str,
        outbound: OutboundDiscordMessage,
    ) -> Result<()> {
        let content = outbound.render_content();
        debug!(
            "sending discord message channel_id={} reply_to={:?} edit_of={:?} attachments={} content_len={} content_preview={}",
            discord_channel_id,
            outbound.reply_to,
            outbound.edit_of,
            outbound.attachments.len(),
            content.len(),
            preview_text(&content)
        );
        self.discord_client
            .send_message_with_metadata(
                discord_channel_id,
                &content,
                &outbound.attachments,
                outbound.reply_to.as_deref(),
                outbound.edit_of.as_deref(),
            )
            .await?;
        debug!(
            "discord message sent channel_id={} content_len={}",
            discord_channel_id,
            content.len()
        );
        Ok(())
    }

    pub async fn send_to_matrix_message(
        &self,
        matrix_room_id: &str,
        discord_sender: &str,
        outbound: OutboundMatrixMessage,
    ) -> Result<String> {
        let body = outbound.render_body();
        debug!(
            "sending matrix message room_id={} sender={} reply_to={:?} edit_of={:?} attachments={} body_len={} body_preview={}",
            matrix_room_id,
            discord_sender,
            outbound.reply_to,
            outbound.edit_of,
            outbound.attachments.len(),
            body.len(),
            preview_text(&body)
        );
        let event_id = self
            .matrix_client
            .send_message_with_metadata(
                matrix_room_id,
                discord_sender,
                &body,
                &outbound.attachments,
                outbound.reply_to.as_deref(),
                outbound.edit_of.as_deref(),
            )
            .await?;
        debug!(
            "matrix message sent room_id={} sender={} body_len={}",
            matrix_room_id,
            discord_sender,
            body.len()
        );
        Ok(event_id)
    }

    pub async fn handle_discord_message_with_context(
        &self,
        ctx: DiscordMessageContext,
    ) -> Result<()> {
        debug!(
            "discord inbound message channel_id={} sender={} reply_to={:?} edit_of={:?} attachments={} content_len={} content_preview={}",
            ctx.channel_id,
            ctx.sender_id,
            ctx.reply_to,
            ctx.edit_of,
            ctx.attachments.len(),
            ctx.content.len(),
            preview_text(&ctx.content)
        );

        let room_mapping = self
            .db_manager
            .room_store()
            .get_room_by_discord_channel(&ctx.channel_id)
            .await?;

        debug!(
            "discord inbound mapping lookup channel_id={} mapped={}",
            ctx.channel_id,
            room_mapping.is_some()
        );

        if self.discord_command_handler.is_command(&ctx.content) {
            debug!(
                "discord inbound command detected channel_id={} sender={} command_preview={}",
                ctx.channel_id,
                ctx.sender_id,
                preview_text(&ctx.content)
            );
            let outcome = self.discord_command_handler.handle(
                &ctx.content,
                room_mapping.is_some(),
                &ctx.permissions,
            );
            self.handle_discord_command_outcome(outcome, &ctx, room_mapping.as_ref())
                .await?;
            return Ok(());
        }

        let Some(mapping) = room_mapping else {
            debug!(
                "discord inbound dropped channel_id={} reason=no_matrix_mapping",
                ctx.channel_id
            );
            return Ok(());
        };

        if let Some(discord_user) = self.discord_client.get_user(&ctx.sender_id).await? {
            let vars = [
                ("id", discord_user.id.as_str()),
                ("tag", discord_user.discriminator.as_str()),
                ("username", discord_user.username.as_str()),
            ];
            let display_name = crate::utils::formatting::apply_pattern_string(
                &self.matrix_client.config().ghosts.username_pattern,
                &vars,
            );
            self.matrix_client
                .ensure_ghost_user_registered(&ctx.sender_id, Some(&display_name))
                .await?;
        } else {
            self.matrix_client
                .ensure_ghost_user_registered(&ctx.sender_id, None)
                .await?;
        }

        let mut outbound = self.message_flow.discord_to_matrix(&DiscordInboundMessage {
            channel_id: ctx.channel_id,
            sender_id: ctx.sender_id.clone(),
            content: ctx.content,
            attachments: ctx.attachments,
            reply_to: ctx.reply_to,
            edit_of: ctx.edit_of,
        });

        let reply_mapping = if let Some(reply_discord_message_id) = outbound.reply_to.clone() {
            self.db_manager
                .message_store()
                .get_by_discord_message_id(&reply_discord_message_id)
                .await?
        } else {
            None
        };

        let edit_mapping = if let Some(edit_discord_message_id) = outbound.edit_of.clone() {
            self.db_manager
                .message_store()
                .get_by_discord_message_id(&edit_discord_message_id)
                .await?
        } else {
            None
        };

        apply_message_relation_mappings(
            &mut outbound,
            reply_mapping.as_ref(),
            edit_mapping.as_ref(),
        );
        debug!(
            "discord->matrix outbound prepared channel_id={} matrix_room={} sender={} reply_to={:?} edit_of={:?} attachments={} body_len={} body_preview={}",
            mapping.discord_channel_id,
            mapping.matrix_room_id,
            ctx.sender_id,
            outbound.reply_to,
            outbound.edit_of,
            outbound.attachments.len(),
            outbound.body.len(),
            preview_text(&outbound.body)
        );
        let matrix_event_id = self
            .send_to_matrix_message(&mapping.matrix_room_id, &ctx.sender_id, outbound)
            .await?;

        if let Some(source_message_id) = ctx.source_message_id {
            self.db_manager
                .message_store()
                .upsert_message_mapping(&MessageMapping {
                    id: 0,
                    discord_message_id: source_message_id,
                    matrix_room_id: mapping.matrix_room_id.clone(),
                    matrix_event_id,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
                .await?;
        }
        Ok(())
    }

    pub async fn handle_discord_message_delete(
        &self,
        _discord_channel_id: &str,
        discord_message_id: &str,
    ) -> Result<()> {
        let Some(link) = self
            .db_manager
            .message_store()
            .get_by_discord_message_id(discord_message_id)
            .await?
        else {
            return Ok(());
        };

        let request = build_discord_delete_redaction_request(&link);

        self.matrix_client
            .redact_message(&request.room_id, &request.event_id, Some(request.reason))
            .await?;
        self.db_manager
            .message_store()
            .delete_by_discord_message_id(discord_message_id)
            .await?;
        Ok(())
    }

    pub async fn handle_discord_typing(
        &self,
        discord_channel_id: &str,
        discord_sender_id: &str,
    ) -> Result<()> {
        if self.matrix_client.config().bridge.disable_typing_notifications {
            return Ok(());
        }

        let room_mapping = self
            .db_manager
            .room_store()
            .get_room_by_discord_channel(discord_channel_id)
            .await?;
        let Some(mapping) = room_mapping else {
            return Ok(());
        };

        self.matrix_client
            .ensure_ghost_user_registered(discord_sender_id, None)
            .await?;

        let request = build_discord_typing_request(&mapping.matrix_room_id, discord_sender_id);

        self.matrix_client
            .set_discord_user_typing(
                &request.room_id,
                &request.discord_user_id,
                request.typing,
                request.timeout_ms,
            )
            .await?;

        debug!(
            "discord typing forwarded channel_id={} sender={} mapped_room={}",
            discord_channel_id,
            discord_sender_id,
            mapping.matrix_room_id
        );

        Ok(())
    }

    async fn handle_discord_command_outcome(
        &self,
        outcome: DiscordCommandOutcome,
        ctx: &DiscordMessageContext,
        room_mapping: Option<&RoomMapping>,
    ) -> Result<()> {
        match outcome {
            DiscordCommandOutcome::Ignored => {}
            DiscordCommandOutcome::Reply(reply) => {
                self.discord_client
                    .send_message(&ctx.channel_id, &reply)
                    .await?;
            }
            DiscordCommandOutcome::ApproveRequested => {
                let reply = match self.provisioning.mark_approval(&ctx.channel_id, true) {
                    ApprovalResponseStatus::Applied => {
                        "Thanks for your response! The matrix bridge has been approved."
                    }
                    ApprovalResponseStatus::Expired => {
                        "Thanks for your response, however it has arrived after the deadline - sorry!"
                    }
                };
                self.discord_client
                    .send_message(&ctx.channel_id, reply)
                    .await?;
            }
            DiscordCommandOutcome::DenyRequested => {
                let reply = match self.provisioning.mark_approval(&ctx.channel_id, false) {
                    ApprovalResponseStatus::Applied => {
                        "Thanks for your response! The matrix bridge has been declined."
                    }
                    ApprovalResponseStatus::Expired => {
                        "Thanks for your response, however it has arrived after the deadline - sorry!"
                    }
                };
                self.discord_client
                    .send_message(&ctx.channel_id, reply)
                    .await?;
            }
            DiscordCommandOutcome::ModerationRequested {
                action,
                matrix_user,
            } => {
                let action_word = match action {
                    ModerationAction::Kick => "Kicked",
                    ModerationAction::Ban => "Banned",
                    ModerationAction::Unban => "Unbanned",
                };
                let reply = format!("{action_word} {matrix_user}");
                self.discord_client
                    .send_message(&ctx.channel_id, &reply)
                    .await?;
                if let Some(mapping) = room_mapping {
                    let notice = format!(
                        "Discord moderation request: {} {} (requested by {})",
                        action_keyword(&action),
                        matrix_user,
                        ctx.sender_id
                    );
                    self.matrix_client
                        .send_notice(&mapping.matrix_room_id, &notice)
                        .await?;
                }
            }
            DiscordCommandOutcome::UnbridgeRequested => {
                if let Some(mapping) = room_mapping {
                    self.db_manager
                        .room_store()
                        .delete_room_mapping(mapping.id)
                        .await?;
                    self.discord_client
                        .send_message(&ctx.channel_id, "This channel has been unbridged")
                        .await?;
                } else {
                    self.discord_client
                        .send_message(
                            &ctx.channel_id,
                            "This channel is not bridged to a plumbed matrix room",
                        )
                        .await?;
                }
            }
        }
        Ok(())
    }

    pub async fn handle_discord_message(
        &self,
        discord_channel_id: &str,
        discord_sender: &str,
        content: &str,
    ) -> Result<()> {
        self.handle_discord_message_with_context(DiscordMessageContext {
            channel_id: discord_channel_id.to_string(),
            source_message_id: None,
            sender_id: discord_sender.to_string(),
            content: content.to_string(),
            attachments: Vec::new(),
            reply_to: None,
            edit_of: None,
            permissions: HashSet::new(),
        })
        .await
    }

    pub fn enqueue_discord_presence(&self, presence: DiscordPresence) {
        self.presence_handler.enqueue_user(presence);
    }

    pub fn db(&self) -> Arc<DatabaseManager> {
        self.db_manager.clone()
    }
}

#[async_trait]
impl MatrixPresenceTarget for MatrixAppservice {
    async fn set_presence(
        &self,
        discord_user_id: &str,
        presence: MatrixPresenceState,
        status_message: &str,
    ) -> Result<()> {
        let presence = match presence {
            MatrixPresenceState::Online => "online",
            MatrixPresenceState::Offline => "offline",
            MatrixPresenceState::Unavailable => "unavailable",
        };
        self.set_discord_user_presence(discord_user_id, presence, status_message)
            .await
    }

    async fn ensure_user_registered(
        &self,
        discord_user_id: &str,
        username: Option<&str>,
    ) -> Result<()> {
        self.ensure_ghost_user_registered(discord_user_id, username)
            .await
    }
}

fn action_keyword(action: &ModerationAction) -> &'static str {
    match action {
        ModerationAction::Kick => "kick",
        ModerationAction::Ban => "ban",
        ModerationAction::Unban => "unban",
    }
}

fn preview_text(value: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 120;
    let mut chars = value.chars();
    let preview: String = chars.by_ref().take(MAX_PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn apply_message_relation_mappings(
    outbound: &mut OutboundMatrixMessage,
    reply_mapping: Option<&MessageMapping>,
    edit_mapping: Option<&MessageMapping>,
) {
    if let Some(link) = reply_mapping {
        outbound.reply_to = Some(link.matrix_event_id.clone());
    }

    if let Some(link) = edit_mapping {
        outbound.edit_of = Some(link.matrix_event_id.clone());
    }
}

fn build_discord_delete_redaction_request(link: &MessageMapping) -> RedactionRequest {
    RedactionRequest {
        room_id: link.matrix_room_id.clone(),
        event_id: link.matrix_event_id.clone(),
        reason: "Deleted on Discord",
    }
}

fn build_discord_typing_request(matrix_room_id: &str, discord_user_id: &str) -> TypingRequest {
    TypingRequest {
        room_id: matrix_room_id.to_string(),
        discord_user_id: discord_user_id.to_string(),
        typing: true,
        timeout_ms: Some(DISCORD_TYPING_TIMEOUT_MS),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{
        apply_message_relation_mappings, build_discord_delete_redaction_request,
        build_discord_typing_request,
        OutboundMatrixMessage,
    };
    use crate::db::MessageMapping;

    fn mapping(discord_message_id: &str, matrix_event_id: &str) -> MessageMapping {
        MessageMapping {
            id: 0,
            discord_message_id: discord_message_id.to_string(),
            matrix_room_id: "!room:example.org".to_string(),
            matrix_event_id: matrix_event_id.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn apply_message_relation_mappings_replaces_ids_when_links_exist() {
        let mut outbound = OutboundMatrixMessage {
            body: "hello".to_string(),
            reply_to: Some("discord-reply-id".to_string()),
            edit_of: Some("discord-edit-id".to_string()),
            attachments: Vec::new(),
        };

        let reply = mapping("discord-reply-id", "$matrix-reply");
        let edit = mapping("discord-edit-id", "$matrix-edit");

        apply_message_relation_mappings(&mut outbound, Some(&reply), Some(&edit));

        assert_eq!(outbound.reply_to, Some("$matrix-reply".to_string()));
        assert_eq!(outbound.edit_of, Some("$matrix-edit".to_string()));
    }

    #[test]
    fn apply_message_relation_mappings_keeps_original_when_links_missing() {
        let mut outbound = OutboundMatrixMessage {
            body: "hello".to_string(),
            reply_to: Some("discord-reply-id".to_string()),
            edit_of: Some("discord-edit-id".to_string()),
            attachments: Vec::new(),
        };

        apply_message_relation_mappings(&mut outbound, None, None);

        assert_eq!(outbound.reply_to, Some("discord-reply-id".to_string()));
        assert_eq!(outbound.edit_of, Some("discord-edit-id".to_string()));
    }

    #[test]
    fn build_discord_delete_redaction_request_maps_fields() {
        let link = mapping("discord-msg-1", "$matrix-event-1");

        let request = build_discord_delete_redaction_request(&link);

        assert_eq!(request.room_id, "!room:example.org");
        assert_eq!(request.event_id, "$matrix-event-1");
        assert_eq!(request.reason, "Deleted on Discord");
    }

    #[test]
    fn build_discord_typing_request_maps_fields() {
        let request = build_discord_typing_request("!room:example.org", "discord-user-1");

        assert_eq!(request.room_id, "!room:example.org");
        assert_eq!(request.discord_user_id, "discord-user-1");
        assert!(request.typing);
        assert_eq!(request.timeout_ms, Some(4000));
    }

    #[test]
    fn build_discord_typing_request_uses_constant_timeout() {
        let request = build_discord_typing_request("!room:example.org", "discord-user-2");
        assert_eq!(request.timeout_ms, Some(super::DISCORD_TYPING_TIMEOUT_MS));
    }
}
