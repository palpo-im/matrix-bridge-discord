// SQLite schema definitions
// This file mirrors schema.rs but uses SQLite-compatible types

diesel::table! {
    room_mappings (id) {
        id -> Integer,
        matrix_room_id -> Text,
        discord_channel_id -> Text,
        discord_channel_name -> Text,
        discord_guild_id -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    user_mappings (id) {
        id -> Integer,
        matrix_user_id -> Text,
        discord_user_id -> Text,
        discord_username -> Text,
        discord_discriminator -> Text,
        discord_avatar -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    processed_events (id) {
        id -> Integer,
        event_id -> Text,
        event_type -> Text,
        source -> Text,
        processed_at -> Text,
    }
}

diesel::table! {
    message_mappings (id) {
        id -> Integer,
        discord_message_id -> Text,
        matrix_room_id -> Text,
        matrix_event_id -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    emoji_mappings (id) {
        id -> Integer,
        discord_emoji_id -> Text,
        emoji_name -> Text,
        animated -> Bool,
        mxc_url -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    room_mappings,
    user_mappings,
    processed_events,
    message_mappings,
    emoji_mappings,
);
