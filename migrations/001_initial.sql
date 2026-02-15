-- 用户映射表
CREATE TABLE IF NOT EXISTS user_mappings (
    id SERIAL PRIMARY KEY,
    matrix_user_id TEXT NOT NULL UNIQUE,
    discord_user_id TEXT NOT NULL UNIQUE,
    discord_username TEXT NOT NULL,
    discord_discriminator TEXT NOT NULL,
    discord_avatar TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- 房间映射表
CREATE TABLE IF NOT EXISTS room_mappings (
    id SERIAL PRIMARY KEY,
    matrix_room_id TEXT NOT NULL UNIQUE,
    discord_channel_id TEXT NOT NULL UNIQUE,
    discord_channel_name TEXT NOT NULL,
    discord_guild_id TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- 事件跟踪表
CREATE TABLE IF NOT EXISTS processed_events (
    id SERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    source TEXT NOT NULL, -- 'matrix' or 'discord'
    processed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- 用户活动表
CREATE TABLE IF NOT EXISTS user_activity (
    id SERIAL PRIMARY KEY,
    user_mapping_id INTEGER NOT NULL REFERENCES user_mappings(id) ON DELETE CASCADE,
    activity_type TEXT NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    metadata JSONB
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_user_mappings_matrix_id ON user_mappings(matrix_user_id);
CREATE INDEX IF NOT EXISTS idx_user_mappings_discord_id ON user_mappings(discord_user_id);
CREATE INDEX IF NOT EXISTS idx_room_mappings_matrix_id ON room_mappings(matrix_room_id);
CREATE INDEX IF NOT EXISTS idx_room_mappings_discord_id ON room_mappings(discord_channel_id);
CREATE INDEX IF NOT EXISTS idx_processed_events_event_id ON processed_events(event_id);
CREATE INDEX IF NOT EXISTS idx_user_activity_user_mapping ON user_activity(user_mapping_id);
CREATE INDEX IF NOT EXISTS idx_user_activity_timestamp ON user_activity(timestamp);