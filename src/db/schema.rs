diesel::table! {
    room_mappings (id) {
        id -> BigInt,
        matrix_room_id -> Text,
        discord_channel_id -> Text,
        discord_channel_name -> Text,
        discord_guild_id -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    user_mappings (id) {
        id -> BigInt,
        matrix_user_id -> Text,
        discord_user_id -> Text,
        discord_username -> Text,
        discord_discriminator -> Text,
        discord_avatar -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    processed_events (id) {
        id -> BigInt,
        event_id -> Text,
        event_type -> Text,
        source -> Text,
        processed_at -> Timestamptz,
    }
}

diesel::table! {
    message_mappings (id) {
        id -> BigInt,
        discord_message_id -> Text,
        matrix_room_id -> Text,
        matrix_event_id -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    room_mappings,
    user_mappings,
    processed_events,
    message_mappings,
);
