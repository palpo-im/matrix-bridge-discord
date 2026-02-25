use std::sync::Arc;
use std::{collections::HashSet, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use tracing::{debug, info};

use crate::db::{DatabaseManager, RoomMapping};
use crate::discord::{
    DiscordClient, DiscordCommandHandler, DiscordCommandOutcome, ModerationAction,
};
use crate::matrix::{MatrixAppservice, MatrixCommandHandler, MatrixCommandOutcome, MatrixEvent};

pub mod message_flow;
pub mod presence_handler;

use self::message_flow::{
    DiscordInboundMessage, MessageFlow, OutboundDiscordMessage, OutboundMatrixMessage,
};
use self::presence_handler::{
    DiscordPresence, MatrixPresenceState, MatrixPresenceTarget, PresenceHandler,
};

#[derive(Debug, Clone)]
pub struct DiscordMessageContext {
    pub channel_id: String,
    pub sender_id: String,
    pub content: String,
    pub attachments: Vec<String>,
    pub reply_to: Option<String>,
    pub edit_of: Option<String>,
    pub permissions: HashSet<String>,
}

#[derive(Clone)]
pub struct BridgeCore {
    matrix_client: Arc<MatrixAppservice>,
    discord_client: Arc<DiscordClient>,
    db_manager: Arc<DatabaseManager>,
    message_flow: Arc<MessageFlow>,
    matrix_command_handler: Arc<MatrixCommandHandler>,
    discord_command_handler: Arc<DiscordCommandHandler>,
    presence_handler: Arc<PresenceHandler>,
}

impl BridgeCore {
    pub fn new(
        matrix_client: Arc<MatrixAppservice>,
        discord_client: Arc<DiscordClient>,
        db_manager: Arc<DatabaseManager>,
    ) -> Self {
        Self {
            message_flow: Arc::new(MessageFlow::new(
                matrix_client.clone(),
                discord_client.clone(),
            )),
            matrix_command_handler: Arc::new(MatrixCommandHandler::default()),
            discord_command_handler: Arc::new(DiscordCommandHandler::new()),
            presence_handler: Arc::new(PresenceHandler::new(None)),
            matrix_client,
            discord_client,
            db_manager,
        }
    }

    pub async fn start(&self) -> Result<()> {
        self.matrix_client.start().await?;
        self.discord_client.start().await?;

        info!("bridge core started");

        let mut ticker = tokio::time::interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            self.presence_handler
                .process_next(self.matrix_client.as_ref())
                .await?;
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
    }

    pub async fn handle_matrix_message(&self, event: &MatrixEvent) -> Result<()> {
        let body = event
            .content
            .as_ref()
            .map(crate::parsers::MessageUtils::extract_plain_text)
            .unwrap_or_default();

        let room_mapping = self
            .db_manager
            .room_store()
            .get_room_by_matrix_room(&event.room_id)
            .await?;

        if self.matrix_command_handler.is_command(&body) {
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
            let outcome = self
                .matrix_command_handler
                .handle(&body, room_mapping.is_some(), |_| Ok(has_permissions));
            self.handle_matrix_command_outcome(outcome, event).await?;
            return Ok(());
        }

        let Some(mapping) = room_mapping else {
            return Ok(());
        };
        let Some(message) = MessageFlow::parse_matrix_event(event) else {
            return Ok(());
        };

        let outbound = self.message_flow.matrix_to_discord(&message);
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
                    .bridge_matrix_room(&event.room_id, &guild_id, &channel_id)
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

    async fn bridge_matrix_room(
        &self,
        matrix_room_id: &str,
        guild_id: &str,
        channel_id: &str,
    ) -> Result<String> {
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
            discord_channel_id: channel.id,
            discord_channel_name: channel.name,
            discord_guild_id: guild_id.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.db_manager
            .room_store()
            .create_room_mapping(&mapping)
            .await?;

        Ok("I have bridged this room to your channel".to_string())
    }

    async fn unbridge_matrix_room(&self, matrix_room_id: &str) -> Result<String> {
        let room_mapping = self
            .db_manager
            .room_store()
            .get_room_by_matrix_room(matrix_room_id)
            .await?;

        let Some(mapping) = room_mapping else {
            return Ok("This room is not bridged.".to_string());
        };
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
        self.discord_client
            .send_message_with_metadata(
                discord_channel_id,
                &content,
                &outbound.attachments,
                outbound.reply_to.as_deref(),
                outbound.edit_of.as_deref(),
            )
            .await?;
        Ok(())
    }

    pub async fn send_to_matrix_message(
        &self,
        matrix_room_id: &str,
        discord_sender: &str,
        outbound: OutboundMatrixMessage,
    ) -> Result<()> {
        let body = outbound.render_body();
        self.matrix_client
            .send_message_with_metadata(
                matrix_room_id,
                discord_sender,
                &body,
                &outbound.attachments,
                outbound.reply_to.as_deref(),
                outbound.edit_of.as_deref(),
            )
            .await?;
        Ok(())
    }

    pub async fn handle_discord_message_with_context(
        &self,
        ctx: DiscordMessageContext,
    ) -> Result<()> {
        let room_mapping = self
            .db_manager
            .room_store()
            .get_room_by_discord_channel(&ctx.channel_id)
            .await?;

        if self.discord_command_handler.is_command(&ctx.content) {
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
            return Ok(());
        };
        let outbound = self.message_flow.discord_to_matrix(&DiscordInboundMessage {
            channel_id: ctx.channel_id,
            sender_id: ctx.sender_id.clone(),
            content: ctx.content,
            attachments: ctx.attachments,
            reply_to: ctx.reply_to,
            edit_of: ctx.edit_of,
        });
        self.send_to_matrix_message(&mapping.matrix_room_id, &ctx.sender_id, outbound)
            .await?;
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
                self.discord_client
                    .send_message(
                        &ctx.channel_id,
                        "Thanks for your response! The matrix bridge has been approved.",
                    )
                    .await?;
            }
            DiscordCommandOutcome::DenyRequested => {
                self.discord_client
                    .send_message(
                        &ctx.channel_id,
                        "Thanks for your response! The matrix bridge has been declined.",
                    )
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
