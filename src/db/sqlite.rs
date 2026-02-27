use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use std::sync::Arc;

use crate::db::schema_sqlite::{room_mappings, user_mappings};

use super::{
    DatabaseError,
    models::{RoomMapping, UserMapping},
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
}
