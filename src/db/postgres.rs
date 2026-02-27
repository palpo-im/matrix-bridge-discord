use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::pg::PgConnection;
use diesel::prelude::*;

use crate::db::manager::Pool;
use crate::db::schema::{message_mappings, room_mappings, user_mappings};

use super::{
    DatabaseError,
    models::{MessageMapping, RoomMapping, UserMapping},
};

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = room_mappings)]
struct DbRoomMapping {
    id: i64,
    matrix_room_id: String,
    discord_channel_id: String,
    discord_channel_name: String,
    discord_guild_id: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DbRoomMapping> for RoomMapping {
    fn from(value: DbRoomMapping) -> Self {
        Self {
            id: value.id,
            matrix_room_id: value.matrix_room_id,
            discord_channel_id: value.discord_channel_id,
            discord_channel_name: value.discord_channel_name,
            discord_guild_id: value.discord_guild_id,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = room_mappings)]
struct NewRoomMapping<'a> {
    matrix_room_id: &'a str,
    discord_channel_id: &'a str,
    discord_channel_name: &'a str,
    discord_guild_id: &'a str,
    created_at: &'a DateTime<Utc>,
    updated_at: &'a DateTime<Utc>,
}

#[derive(AsChangeset)]
#[diesel(table_name = room_mappings)]
struct UpdateRoomMapping<'a> {
    matrix_room_id: &'a str,
    discord_channel_id: &'a str,
    discord_channel_name: &'a str,
    discord_guild_id: &'a str,
    updated_at: &'a DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = user_mappings)]
struct DbUserMapping {
    id: i64,
    matrix_user_id: String,
    discord_user_id: String,
    discord_username: String,
    discord_discriminator: String,
    discord_avatar: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DbUserMapping> for UserMapping {
    fn from(value: DbUserMapping) -> Self {
        Self {
            id: value.id,
            matrix_user_id: value.matrix_user_id,
            discord_user_id: value.discord_user_id,
            discord_username: value.discord_username,
            discord_discriminator: value.discord_discriminator,
            discord_avatar: value.discord_avatar,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
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
    created_at: &'a DateTime<Utc>,
    updated_at: &'a DateTime<Utc>,
}

#[derive(AsChangeset)]
#[diesel(table_name = user_mappings)]
struct UpdateUserMapping<'a> {
    discord_username: &'a str,
    discord_discriminator: &'a str,
    discord_avatar: Option<&'a str>,
    updated_at: &'a DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = message_mappings)]
struct DbMessageMapping {
    id: i64,
    discord_message_id: String,
    matrix_room_id: String,
    matrix_event_id: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DbMessageMapping> for MessageMapping {
    fn from(value: DbMessageMapping) -> Self {
        Self {
            id: value.id,
            discord_message_id: value.discord_message_id,
            matrix_room_id: value.matrix_room_id,
            matrix_event_id: value.matrix_event_id,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = message_mappings)]
struct NewMessageMapping<'a> {
    discord_message_id: &'a str,
    matrix_room_id: &'a str,
    matrix_event_id: &'a str,
    created_at: &'a DateTime<Utc>,
    updated_at: &'a DateTime<Utc>,
}

#[derive(AsChangeset)]
#[diesel(table_name = message_mappings)]
struct UpdateMessageMapping<'a> {
    matrix_room_id: &'a str,
    matrix_event_id: &'a str,
    updated_at: &'a DateTime<Utc>,
}

async fn with_connection<T, F>(pool: Pool, operation: F) -> Result<T, DatabaseError>
where
    T: Send + 'static,
    F: FnOnce(&mut PgConnection) -> Result<T, DatabaseError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut conn = pool
            .get()
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        operation(&mut conn)
    })
    .await
    .map_err(|e| DatabaseError::Query(format!("database task failed: {e}")))?
}

pub struct PostgresRoomStore {
    pool: Pool,
}

impl PostgresRoomStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl super::RoomStore for PostgresRoomStore {
    async fn get_room_by_discord_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<RoomMapping>, DatabaseError> {
        let pool = self.pool.clone();
        let channel_id = channel_id.to_string();
        with_connection(pool, move |conn| {
            use crate::db::schema::room_mappings::dsl::*;
            room_mappings
                .filter(discord_channel_id.eq(channel_id))
                .select(DbRoomMapping::as_select())
                .first::<DbRoomMapping>(conn)
                .optional()
                .map(|value| value.map(Into::into))
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn get_room_by_matrix_room(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomMapping>, DatabaseError> {
        let pool = self.pool.clone();
        let room_id = room_id.to_string();
        with_connection(pool, move |conn| {
            use crate::db::schema::room_mappings::dsl::*;
            room_mappings
                .filter(matrix_room_id.eq(room_id))
                .select(DbRoomMapping::as_select())
                .first::<DbRoomMapping>(conn)
                .optional()
                .map(|value| value.map(Into::into))
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn get_room_by_id(&self, mapping_id: i64) -> Result<Option<RoomMapping>, DatabaseError> {
        let pool = self.pool.clone();
        with_connection(pool, move |conn| {
            use crate::db::schema::room_mappings::dsl::*;
            room_mappings
                .filter(id.eq(mapping_id))
                .select(DbRoomMapping::as_select())
                .first::<DbRoomMapping>(conn)
                .optional()
                .map(|value| value.map(Into::into))
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn count_rooms(&self) -> Result<i64, DatabaseError> {
        let pool = self.pool.clone();
        with_connection(pool, move |conn| {
            use crate::db::schema::room_mappings::dsl::*;
            room_mappings
                .count()
                .get_result(conn)
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn list_room_mappings(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RoomMapping>, DatabaseError> {
        let pool = self.pool.clone();
        with_connection(pool, move |conn| {
            use crate::db::schema::room_mappings::dsl::*;
            room_mappings
                .order(id.desc())
                .limit(limit)
                .offset(offset)
                .select(DbRoomMapping::as_select())
                .load::<DbRoomMapping>(conn)
                .map(|rows| rows.into_iter().map(Into::into).collect())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn create_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        let mapping = mapping.clone();
        with_connection(pool, move |conn| {
            let new_mapping = NewRoomMapping {
                matrix_room_id: &mapping.matrix_room_id,
                discord_channel_id: &mapping.discord_channel_id,
                discord_channel_name: &mapping.discord_channel_name,
                discord_guild_id: &mapping.discord_guild_id,
                created_at: &mapping.created_at,
                updated_at: &mapping.updated_at,
            };

            diesel::insert_into(room_mappings::table)
                .values(&new_mapping)
                .execute(conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn update_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        let mapping = mapping.clone();
        with_connection(pool, move |conn| {
            let changes = UpdateRoomMapping {
                matrix_room_id: &mapping.matrix_room_id,
                discord_channel_id: &mapping.discord_channel_id,
                discord_channel_name: &mapping.discord_channel_name,
                discord_guild_id: &mapping.discord_guild_id,
                updated_at: &mapping.updated_at,
            };

            diesel::update(room_mappings::table.filter(room_mappings::id.eq(mapping.id)))
                .set(changes)
                .execute(conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn delete_room_mapping(&self, id: i64) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        with_connection(pool, move |conn| {
            diesel::delete(room_mappings::table.filter(room_mappings::id.eq(id)))
                .execute(conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }
}

pub struct PostgresUserStore {
    pool: Pool,
}

impl PostgresUserStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl super::UserStore for PostgresUserStore {
    async fn get_user_by_discord_id(
        &self,
        discord_id: &str,
    ) -> Result<Option<UserMapping>, DatabaseError> {
        let pool = self.pool.clone();
        let discord_id = discord_id.to_string();
        with_connection(pool, move |conn| {
            use crate::db::schema::user_mappings::dsl::*;
            user_mappings
                .filter(discord_user_id.eq(discord_id))
                .select(DbUserMapping::as_select())
                .first::<DbUserMapping>(conn)
                .optional()
                .map(|value| value.map(Into::into))
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn create_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        let mapping = mapping.clone();
        with_connection(pool, move |conn| {
            let new_mapping = NewUserMapping {
                matrix_user_id: &mapping.matrix_user_id,
                discord_user_id: &mapping.discord_user_id,
                discord_username: &mapping.discord_username,
                discord_discriminator: &mapping.discord_discriminator,
                discord_avatar: mapping.discord_avatar.as_deref(),
                created_at: &mapping.created_at,
                updated_at: &mapping.updated_at,
            };

            diesel::insert_into(user_mappings::table)
                .values(new_mapping)
                .execute(conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn update_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        let mapping = mapping.clone();
        with_connection(pool, move |conn| {
            let changes = UpdateUserMapping {
                discord_username: &mapping.discord_username,
                discord_discriminator: &mapping.discord_discriminator,
                discord_avatar: mapping.discord_avatar.as_deref(),
                updated_at: &mapping.updated_at,
            };

            diesel::update(user_mappings::table.filter(user_mappings::id.eq(mapping.id)))
                .set(changes)
                .execute(conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn delete_user_mapping(&self, id: i64) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        with_connection(pool, move |conn| {
            diesel::delete(user_mappings::table.filter(user_mappings::id.eq(id)))
                .execute(conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }
}

pub struct PostgresMessageStore {
    pool: Pool,
}

impl PostgresMessageStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl super::MessageStore for PostgresMessageStore {
    async fn get_by_discord_message_id(
        &self,
        discord_message_id_param: &str,
    ) -> Result<Option<MessageMapping>, DatabaseError> {
        let pool = self.pool.clone();
        let discord_message_id_param = discord_message_id_param.to_string();
        with_connection(pool, move |conn| {
            use crate::db::schema::message_mappings::dsl::*;
            message_mappings
                .filter(discord_message_id.eq(discord_message_id_param))
                .select(DbMessageMapping::as_select())
                .first::<DbMessageMapping>(conn)
                .optional()
                .map(|value| value.map(Into::into))
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn get_by_matrix_event_id(
        &self,
        matrix_event_id_param: &str,
    ) -> Result<Option<MessageMapping>, DatabaseError> {
        let pool = self.pool.clone();
        let matrix_event_id_param = matrix_event_id_param.to_string();
        with_connection(pool, move |conn| {
            use crate::db::schema::message_mappings::dsl::*;
            message_mappings
                .filter(matrix_event_id.eq(matrix_event_id_param))
                .select(DbMessageMapping::as_select())
                .first::<DbMessageMapping>(conn)
                .optional()
                .map(|value| value.map(Into::into))
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }

    async fn upsert_message_mapping(&self, mapping: &MessageMapping) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        let mapping = mapping.clone();
        with_connection(pool, move |conn| {
            use crate::db::schema::message_mappings::dsl::*;

            let existing = message_mappings
                .filter(discord_message_id.eq(&mapping.discord_message_id))
                .select(DbMessageMapping::as_select())
                .first::<DbMessageMapping>(conn)
                .optional()
                .map_err(|e| DatabaseError::Query(e.to_string()))?;

            if let Some(existing) = existing {
                let changes = UpdateMessageMapping {
                    matrix_room_id: &mapping.matrix_room_id,
                    matrix_event_id: &mapping.matrix_event_id,
                    updated_at: &mapping.updated_at,
                };
                diesel::update(message_mappings.filter(id.eq(existing.id)))
                    .set(changes)
                    .execute(conn)
                    .map(|_| ())
                    .map_err(|e| DatabaseError::Query(e.to_string()))
            } else {
                let new_mapping = NewMessageMapping {
                    discord_message_id: &mapping.discord_message_id,
                    matrix_room_id: &mapping.matrix_room_id,
                    matrix_event_id: &mapping.matrix_event_id,
                    created_at: &mapping.created_at,
                    updated_at: &mapping.updated_at,
                };
                diesel::insert_into(message_mappings)
                    .values(new_mapping)
                    .execute(conn)
                    .map(|_| ())
                    .map_err(|e| DatabaseError::Query(e.to_string()))
            }
        })
        .await
    }

    async fn delete_by_discord_message_id(
        &self,
        discord_message_id_param: &str,
    ) -> Result<(), DatabaseError> {
        let pool = self.pool.clone();
        let discord_message_id_param = discord_message_id_param.to_string();
        with_connection(pool, move |conn| {
            use crate::db::schema::message_mappings::dsl::*;
            diesel::delete(message_mappings.filter(discord_message_id.eq(discord_message_id_param)))
                .execute(conn)
                .map(|_| ())
                .map_err(|e| DatabaseError::Query(e.to_string()))
        })
        .await
    }
}
