use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use std::sync::Arc;

use crate::db::schema_sqlite::{message_mappings, room_mappings, user_mappings};

use super::{
    DatabaseError,
    models::{EmojiMapping, MessageMapping, RemoteRoomInfo, RemoteUserInfo, RoomMapping, UserMapping},
};

// Helper function to convert DateTime to ISO string for SQLite
fn datetime_to_string(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

// Helper function to parse ISO string to DateTime
fn string_to_datetime(s: &str) -> Result<DateTime<Utc>, DatabaseError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| DatabaseError::Query(format!("invalid datetime format: {}", e)))
}

// SQLite uses i32 for INTEGER (primary keys), but we want to keep i64 in our API
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = room_mappings)]
struct DbRoomMapping {
    id: i32,
    matrix_room_id: String,
    discord_channel_id: String,
    discord_channel_name: String,
    discord_guild_id: String,
    created_at: String,
    updated_at: String,
}

impl DbRoomMapping {
    fn to_room_mapping(&self) -> Result<RoomMapping, DatabaseError> {
        Ok(RoomMapping {
            id: self.id as i64,
            matrix_room_id: self.matrix_room_id.clone(),
            discord_channel_id: self.discord_channel_id.clone(),
            discord_channel_name: self.discord_channel_name.clone(),
            discord_guild_id: self.discord_guild_id.clone(),
            created_at: string_to_datetime(&self.created_at)?,
            updated_at: string_to_datetime(&self.updated_at)?,
        })
    }
}

#[derive(Insertable)]
#[diesel(table_name = room_mappings)]
struct NewRoomMapping<'a> {
    matrix_room_id: &'a str,
    discord_channel_id: &'a str,
    discord_channel_name: &'a str,
    discord_guild_id: &'a str,
    created_at: String,
    updated_at: String,
}

#[derive(AsChangeset)]
#[diesel(table_name = room_mappings)]
struct UpdateRoomMapping<'a> {
    matrix_room_id: &'a str,
    discord_channel_id: &'a str,
    discord_channel_name: &'a str,
    discord_guild_id: &'a str,
    updated_at: String,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = user_mappings)]
struct DbUserMapping {
    id: i32,
    matrix_user_id: String,
    discord_user_id: String,
    discord_username: String,
    discord_discriminator: String,
    discord_avatar: Option<String>,
    created_at: String,
    updated_at: String,
}

impl DbUserMapping {
    fn to_user_mapping(&self) -> Result<UserMapping, DatabaseError> {
        Ok(UserMapping {
            id: self.id as i64,
            matrix_user_id: self.matrix_user_id.clone(),
            discord_user_id: self.discord_user_id.clone(),
            discord_username: self.discord_username.clone(),
            discord_discriminator: self.discord_discriminator.clone(),
            discord_avatar: self.discord_avatar.clone(),
            created_at: string_to_datetime(&self.created_at)?,
            updated_at: string_to_datetime(&self.updated_at)?,
        })
    }
}

#[derive(Insertable)]
#[diesel(table_name = user_mappings)]
struct NewUserMapping<'a> {
    matrix_user_id: &'a str,
    discord_user_id: &'a str,
    discord_username: &'a str,
    discord_discriminator: &'a str,
    discord_avatar: Option<&'a str>,
    created_at: String,
    updated_at: String,
}

#[derive(AsChangeset)]
#[diesel(table_name = user_mappings)]
struct UpdateUserMapping<'a> {
    discord_username: &'a str,
    discord_discriminator: &'a str,
    discord_avatar: Option<&'a str>,
    updated_at: String,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = message_mappings)]
struct DbMessageMapping {
    id: i32,
    discord_message_id: String,
    matrix_room_id: String,
    matrix_event_id: String,
    created_at: String,
    updated_at: String,
}

impl DbMessageMapping {
    fn to_message_mapping(&self) -> Result<MessageMapping, DatabaseError> {
        Ok(MessageMapping {
            id: self.id as i64,
            discord_message_id: self.discord_message_id.clone(),
            matrix_room_id: self.matrix_room_id.clone(),
            matrix_event_id: self.matrix_event_id.clone(),
            created_at: string_to_datetime(&self.created_at)?,
            updated_at: string_to_datetime(&self.updated_at)?,
        })
    }
}

#[derive(Insertable)]
#[diesel(table_name = message_mappings)]
struct NewMessageMapping<'a> {
    discord_message_id: &'a str,
    matrix_room_id: &'a str,
    matrix_event_id: &'a str,
    created_at: String,
    updated_at: String,
}

#[derive(AsChangeset)]
#[diesel(table_name = message_mappings)]
struct UpdateMessageMapping<'a> {
    matrix_room_id: &'a str,
    matrix_event_id: &'a str,
    updated_at: String,
}

fn establish_connection(path: &str) -> Result<SqliteConnection, DatabaseError> {
    SqliteConnection::establish(path)
        .map_err(|e| DatabaseError::Connection(e.to_string()))
}

pub struct SqliteRoomStore {
    db_path: Arc<String>,
}

impl SqliteRoomStore {
    pub fn new(db_path: Arc<String>) -> Self {
        Self { db_path }
    }
}

#[async_trait]
impl super::RoomStore for SqliteRoomStore {
    async fn get_room_by_discord_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<RoomMapping>, DatabaseError> {
        let channel_id = channel_id.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::room_mappings::dsl::*;
            room_mappings
                .filter(discord_channel_id.eq(channel_id))
                .select(DbRoomMapping::as_select())
                .first::<DbRoomMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?
                .map(|m| m.to_room_mapping())
                .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_room_by_matrix_room(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomMapping>, DatabaseError> {
        let room_id = room_id.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::room_mappings::dsl::*;
            room_mappings
                .filter(matrix_room_id.eq(room_id))
                .select(DbRoomMapping::as_select())
                .first::<DbRoomMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?
                .map(|m| m.to_room_mapping())
                .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_room_by_id(&self, mapping_id: i64) -> Result<Option<RoomMapping>, DatabaseError> {
        let mapping_id = mapping_id as i32;
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::room_mappings::dsl::*;
            room_mappings
                .filter(id.eq(mapping_id))
                .select(DbRoomMapping::as_select())
                .first::<DbRoomMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?
                .map(|m| m.to_room_mapping())
                .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn count_rooms(&self) -> Result<i64, DatabaseError> {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::room_mappings::dsl::*;
            room_mappings
                .count()
                .get_result(&mut conn)
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn list_room_mappings(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RoomMapping>, DatabaseError> {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::room_mappings::dsl::*;
            let results = room_mappings
                .order(id.desc())
                .limit(limit)
                .offset(offset)
                .select(DbRoomMapping::as_select())
                .load::<DbRoomMapping>(&mut conn)
                .map_err(|e| DatabaseError::Query(e.to_string()))?;

            results.into_iter().map(|m| m.to_room_mapping()).collect()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn create_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError> {
        let mapping = mapping.clone();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            let new_mapping = NewRoomMapping {
                matrix_room_id: &mapping.matrix_room_id,
                discord_channel_id: &mapping.discord_channel_id,
                discord_channel_name: &mapping.discord_channel_name,
                discord_guild_id: &mapping.discord_guild_id,
                created_at: datetime_to_string(&mapping.created_at),
                updated_at: datetime_to_string(&mapping.updated_at),
            };

            diesel::insert_into(room_mappings::table)
                .values(&new_mapping)
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn update_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError> {
        let mapping = mapping.clone();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            let changes = UpdateRoomMapping {
                matrix_room_id: &mapping.matrix_room_id,
                discord_channel_id: &mapping.discord_channel_id,
                discord_channel_name: &mapping.discord_channel_name,
                discord_guild_id: &mapping.discord_guild_id,
                updated_at: datetime_to_string(&mapping.updated_at),
            };

            diesel::update(room_mappings::table.filter(room_mappings::id.eq(mapping.id as i32)))
                .set(changes)
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn delete_room_mapping(&self, id: i64) -> Result<(), DatabaseError> {
        let id = id as i32;
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            diesel::delete(room_mappings::table.filter(room_mappings::id.eq(id)))
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_rooms_by_guild(
        &self,
        guild_id: &str,
    ) -> Result<Vec<RoomMapping>, DatabaseError> {
        let guild_id = guild_id.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::room_mappings::dsl::*;
            let results = room_mappings
                .filter(discord_guild_id.eq(guild_id))
                .select(DbRoomMapping::as_select())
                .load::<DbRoomMapping>(&mut conn)
                .map_err(|e| DatabaseError::Query(e.to_string()))?;
            results.into_iter().map(|m| m.to_room_mapping()).collect()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_remote_room_info(
        &self,
        _matrix_room_id: &str,
    ) -> Result<Option<RemoteRoomInfo>, DatabaseError> {
        Ok(None)
    }

    async fn update_remote_room_info(
        &self,
        _matrix_room_id: &str,
        _info: &RemoteRoomInfo,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
}

pub struct SqliteUserStore {
    db_path: Arc<String>,
}

impl SqliteUserStore {
    pub fn new(db_path: Arc<String>) -> Self {
        Self { db_path }
    }
}

#[async_trait]
impl super::UserStore for SqliteUserStore {
    async fn get_user_by_discord_id(
        &self,
        discord_id: &str,
    ) -> Result<Option<UserMapping>, DatabaseError> {
        let discord_id = discord_id.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::user_mappings::dsl::*;
            user_mappings
                .filter(discord_user_id.eq(discord_id))
                .select(DbUserMapping::as_select())
                .first::<DbUserMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?
                .map(|m| m.to_user_mapping())
                .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn create_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError> {
        let mapping = mapping.clone();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            let new_mapping = NewUserMapping {
                matrix_user_id: &mapping.matrix_user_id,
                discord_user_id: &mapping.discord_user_id,
                discord_username: &mapping.discord_username,
                discord_discriminator: &mapping.discord_discriminator,
                discord_avatar: mapping.discord_avatar.as_deref(),
                created_at: datetime_to_string(&mapping.created_at),
                updated_at: datetime_to_string(&mapping.updated_at),
            };

            diesel::insert_into(user_mappings::table)
                .values(new_mapping)
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn update_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError> {
        let mapping = mapping.clone();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            let changes = UpdateUserMapping {
                discord_username: &mapping.discord_username,
                discord_discriminator: &mapping.discord_discriminator,
                discord_avatar: mapping.discord_avatar.as_deref(),
                updated_at: datetime_to_string(&mapping.updated_at),
            };

            diesel::update(user_mappings::table.filter(user_mappings::id.eq(mapping.id as i32)))
                .set(changes)
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn delete_user_mapping(&self, id: i64) -> Result<(), DatabaseError> {
        let id = id as i32;
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            diesel::delete(user_mappings::table.filter(user_mappings::id.eq(id)))
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_user_by_matrix_id(
        &self,
        matrix_id: &str,
    ) -> Result<Option<UserMapping>, DatabaseError> {
        let matrix_id = matrix_id.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::user_mappings::dsl::*;
            user_mappings
                .filter(matrix_user_id.eq(matrix_id))
                .select(DbUserMapping::as_select())
                .first::<DbUserMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?
                .map(|m| m.to_user_mapping())
                .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_remote_user_info(
        &self,
        _discord_user_id: &str,
    ) -> Result<Option<RemoteUserInfo>, DatabaseError> {
        Ok(None)
    }

    async fn update_remote_user_info(
        &self,
        _discord_user_id: &str,
        _info: &RemoteUserInfo,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn get_all_user_ids(&self) -> Result<Vec<String>, DatabaseError> {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::user_mappings::dsl::*;
            user_mappings
                .select(matrix_user_id)
                .load::<String>(&mut conn)
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }
}

pub struct SqliteMessageStore {
    db_path: Arc<String>,
}

impl SqliteMessageStore {
    pub fn new(db_path: Arc<String>) -> Self {
        Self { db_path }
    }
}

#[async_trait]
impl super::MessageStore for SqliteMessageStore {
    async fn get_by_discord_message_id(
        &self,
        discord_message_id_param: &str,
    ) -> Result<Option<MessageMapping>, DatabaseError> {
        let discord_message_id_param = discord_message_id_param.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::message_mappings::dsl::*;
            message_mappings
                .filter(discord_message_id.eq(discord_message_id_param))
                .select(DbMessageMapping::as_select())
                .first::<DbMessageMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?
                .map(|m| m.to_message_mapping())
                .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_by_matrix_event_id(
        &self,
        matrix_event_id_param: &str,
    ) -> Result<Option<MessageMapping>, DatabaseError> {
        let matrix_event_id_param = matrix_event_id_param.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::message_mappings::dsl::*;
            message_mappings
                .filter(matrix_event_id.eq(matrix_event_id_param))
                .select(DbMessageMapping::as_select())
                .first::<DbMessageMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?
                .map(|m| m.to_message_mapping())
                .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn upsert_message_mapping(&self, mapping: &MessageMapping) -> Result<(), DatabaseError> {
        let mapping = mapping.clone();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::message_mappings::dsl::*;

            let existing = message_mappings
                .filter(discord_message_id.eq(&mapping.discord_message_id))
                .select(DbMessageMapping::as_select())
                .first::<DbMessageMapping>(&mut conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?;

            if let Some(existing) = existing {
                let changes = UpdateMessageMapping {
                    matrix_room_id: &mapping.matrix_room_id,
                    matrix_event_id: &mapping.matrix_event_id,
                    updated_at: datetime_to_string(&mapping.updated_at),
                };

                diesel::update(message_mappings.filter(id.eq(existing.id)))
                    .set(changes)
                    .execute(&mut conn)
                    .map(|_| ())
                    .map_err(|e| DatabaseError::Query(e.to_string()))
            } else {
                let new_mapping = NewMessageMapping {
                    discord_message_id: &mapping.discord_message_id,
                    matrix_room_id: &mapping.matrix_room_id,
                    matrix_event_id: &mapping.matrix_event_id,
                    created_at: datetime_to_string(&mapping.created_at),
                    updated_at: datetime_to_string(&mapping.updated_at),
                };

                diesel::insert_into(message_mappings)
                    .values(new_mapping)
                    .execute(&mut conn)
                    .map(|_| ())
                    .map_err(|e| DatabaseError::Query(e.to_string()))
            }
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn delete_by_discord_message_id(
        &self,
        discord_message_id_param: &str,
    ) -> Result<(), DatabaseError> {
        let discord_message_id_param = discord_message_id_param.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::message_mappings::dsl::*;
            diesel::delete(message_mappings.filter(discord_message_id.eq(discord_message_id_param)))
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn delete_by_matrix_event_id(
        &self,
        matrix_event_id_param: &str,
    ) -> Result<(), DatabaseError> {
        let matrix_event_id_param = matrix_event_id_param.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            use crate::db::schema_sqlite::message_mappings::dsl::*;
            diesel::delete(message_mappings.filter(matrix_event_id.eq(matrix_event_id_param)))
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }
}

pub struct SqliteEmojiStore {
    db_path: Arc<String>,
}

impl SqliteEmojiStore {
    pub fn new(db_path: Arc<String>) -> Self {
        Self { db_path }
    }
}

#[derive(Debug, Clone, QueryableByName)]
#[diesel(table_name = crate::db::schema_sqlite::emoji_mappings)]
struct DbEmojiMapping {
    id: i32,
    discord_emoji_id: String,
    emoji_name: String,
    animated: bool,
    mxc_url: String,
    created_at: String,
    updated_at: String,
}

impl DbEmojiMapping {
    fn to_emoji_mapping(&self) -> Result<EmojiMapping, DatabaseError> {
        Ok(EmojiMapping {
            id: self.id as i64,
            discord_emoji_id: self.discord_emoji_id.clone(),
            emoji_name: self.emoji_name.clone(),
            animated: self.animated,
            mxc_url: self.mxc_url.clone(),
            created_at: string_to_datetime(&self.created_at)?,
            updated_at: string_to_datetime(&self.updated_at)?,
        })
    }
}

#[async_trait]
impl super::EmojiStore for SqliteEmojiStore {
    async fn get_emoji_by_discord_id(
        &self,
        discord_emoji_id: &str,
    ) -> Result<Option<EmojiMapping>, DatabaseError> {
        let discord_emoji_id = discord_emoji_id.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            diesel::sql_query(
                "SELECT id, discord_emoji_id, emoji_name, animated, mxc_url, created_at, updated_at FROM emoji_mappings WHERE discord_emoji_id = ?"
            )
            .bind::<diesel::sql_types::Text, _>(&discord_emoji_id)
            .get_result::<DbEmojiMapping>(&mut conn)
            .optional()
            .map_err(|e| DatabaseError::Query(e.to_string()))?
            .map(|m| m.to_emoji_mapping())
            .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn get_emoji_by_mxc(
        &self,
        mxc_url: &str,
    ) -> Result<Option<EmojiMapping>, DatabaseError> {
        let mxc_url = mxc_url.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            diesel::sql_query(
                "SELECT id, discord_emoji_id, emoji_name, animated, mxc_url, created_at, updated_at FROM emoji_mappings WHERE mxc_url = ?"
            )
            .bind::<diesel::sql_types::Text, _>(&mxc_url)
            .get_result::<DbEmojiMapping>(&mut conn)
            .optional()
            .map_err(|e| DatabaseError::Query(e.to_string()))?
            .map(|m| m.to_emoji_mapping())
            .transpose()
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn create_emoji(&self, emoji: &EmojiMapping) -> Result<(), DatabaseError> {
        let emoji = emoji.clone();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            diesel::sql_query(
                "INSERT INTO emoji_mappings (discord_emoji_id, emoji_name, animated, mxc_url, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind::<diesel::sql_types::Text, _>(&emoji.discord_emoji_id)
            .bind::<diesel::sql_types::Text, _>(&emoji.emoji_name)
            .bind::<diesel::sql_types::Bool, _>(emoji.animated)
            .bind::<diesel::sql_types::Text, _>(&emoji.mxc_url)
            .bind::<diesel::sql_types::Text, _>(&datetime_to_string(&emoji.created_at))
            .bind::<diesel::sql_types::Text, _>(&datetime_to_string(&emoji.updated_at))
            .execute(&mut conn)
            .map(|_| ())
            .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn update_emoji(&self, emoji: &EmojiMapping) -> Result<(), DatabaseError> {
        let emoji = emoji.clone();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            diesel::sql_query(
                "UPDATE emoji_mappings SET emoji_name = ?, animated = ?, mxc_url = ?, updated_at = ? WHERE discord_emoji_id = ?"
            )
            .bind::<diesel::sql_types::Text, _>(&emoji.emoji_name)
            .bind::<diesel::sql_types::Bool, _>(emoji.animated)
            .bind::<diesel::sql_types::Text, _>(&emoji.mxc_url)
            .bind::<diesel::sql_types::Text, _>(&datetime_to_string(&emoji.updated_at))
            .bind::<diesel::sql_types::Text, _>(&emoji.discord_emoji_id)
            .execute(&mut conn)
            .map(|_| ())
            .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }

    async fn delete_emoji(&self, discord_emoji_id: &str) -> Result<(), DatabaseError> {
        let discord_emoji_id = discord_emoji_id.to_string();
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = establish_connection(&db_path)?;
            diesel::sql_query("DELETE FROM emoji_mappings WHERE discord_emoji_id = ?")
                .bind::<diesel::sql_types::Text, _>(&discord_emoji_id)
                .execute(&mut conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
        .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
    }
}
