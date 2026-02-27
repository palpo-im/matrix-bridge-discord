use crate::db::{MessageMapping, RoomMapping};

use super::message_flow::OutboundMatrixMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RedactionRequest {
    pub(crate) room_id: String,
    pub(crate) event_id: String,
    pub(crate) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypingRequest {
    pub(crate) room_id: String,
    pub(crate) discord_user_id: String,
    pub(crate) typing: bool,
    pub(crate) timeout_ms: Option<u64>,
}

pub(crate) const DISCORD_TYPING_TIMEOUT_MS: u64 = 4000;

pub(crate) fn apply_message_relation_mappings(
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

pub(crate) fn build_discord_delete_redaction_request(link: &MessageMapping) -> RedactionRequest {
    RedactionRequest {
        room_id: link.matrix_room_id.clone(),
        event_id: link.matrix_event_id.clone(),
        reason: "Deleted on Discord",
    }
}

pub(crate) fn discord_delete_redaction_request(
    link: Option<&MessageMapping>,
) -> Option<RedactionRequest> {
    link.map(build_discord_delete_redaction_request)
}

pub(crate) fn build_discord_typing_request(
    matrix_room_id: &str,
    discord_user_id: &str,
) -> TypingRequest {
    TypingRequest {
        room_id: matrix_room_id.to_string(),
        discord_user_id: discord_user_id.to_string(),
        typing: true,
        timeout_ms: Some(DISCORD_TYPING_TIMEOUT_MS),
    }
}

pub(crate) fn should_forward_discord_typing(
    disable_typing_notifications: bool,
    room_mapping: Option<&RoomMapping>,
) -> bool {
    !disable_typing_notifications && room_mapping.is_some()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{
        apply_message_relation_mappings, build_discord_delete_redaction_request,
        build_discord_typing_request, discord_delete_redaction_request,
        should_forward_discord_typing, OutboundMatrixMessage,
    };
    use crate::db::{MessageMapping, RoomMapping};

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
    fn discord_delete_redaction_request_returns_none_without_mapping() {
        let request = discord_delete_redaction_request(None);
        assert!(request.is_none());
    }

    #[test]
    fn discord_delete_redaction_request_returns_some_with_mapping() {
        let link = mapping("discord-msg-2", "$matrix-event-2");

        let request = discord_delete_redaction_request(Some(&link))
            .expect("request should be created when mapping exists");

        assert_eq!(request.room_id, "!room:example.org");
        assert_eq!(request.event_id, "$matrix-event-2");
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

    fn room_mapping() -> RoomMapping {
        RoomMapping {
            id: 1,
            matrix_room_id: "!room:example.org".to_string(),
            discord_channel_id: "123".to_string(),
            discord_channel_name: "general".to_string(),
            discord_guild_id: "456".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn should_forward_discord_typing_returns_false_when_disabled() {
        let mapping = room_mapping();
        let should_forward = should_forward_discord_typing(true, Some(&mapping));
        assert!(!should_forward);
    }

    #[test]
    fn should_forward_discord_typing_returns_false_without_mapping() {
        let should_forward = should_forward_discord_typing(false, None);
        assert!(!should_forward);
    }

    #[test]
    fn should_forward_discord_typing_returns_true_when_enabled_and_mapped() {
        let mapping = room_mapping();
        let should_forward = should_forward_discord_typing(false, Some(&mapping));
        assert!(should_forward);
    }
}
