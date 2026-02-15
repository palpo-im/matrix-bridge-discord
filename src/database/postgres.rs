use async_trait::async_trait;
use sqlx::{PgPool, Row};

use super::{
    models::{RoomMapping, UserMapping},
    DatabaseError,
};

pub struct PostgresRoomStore {
    pool: PgPool,
}

impl PostgresRoomStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_room_mapping(row: sqlx::postgres::PgRow) -> RoomMapping {
        RoomMapping {
            id: row.get("id"),
            matrix_room_id: row.get("matrix_room_id"),
            discord_channel_id: row.get("discord_channel_id"),
            discord_channel_name: row.get("discord_channel_name"),
            discord_guild_id: row.get("discord_guild_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }
}

#[async_trait]
impl super::RoomStore for PostgresRoomStore {
    async fn get_room_by_discord_channel(&self, channel_id: &str) -> Result<Option<RoomMapping>, DatabaseError> {
        let row = sqlx::query(
            "SELECT id, matrix_room_id, discord_channel_id, discord_channel_name, discord_guild_id, created_at, updated_at FROM room_mappings WHERE discord_channel_id = $1",
        )
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(row.map(Self::row_to_room_mapping))
    }

    async fn get_room_by_matrix_room(&self, room_id: &str) -> Result<Option<RoomMapping>, DatabaseError> {
        let row = sqlx::query(
            "SELECT id, matrix_room_id, discord_channel_id, discord_channel_name, discord_guild_id, created_at, updated_at FROM room_mappings WHERE matrix_room_id = $1",
        )
        .bind(room_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(row.map(Self::row_to_room_mapping))
    }

    async fn get_room_by_id(&self, id: i64) -> Result<Option<RoomMapping>, DatabaseError> {
        let row = sqlx::query(
            "SELECT id, matrix_room_id, discord_channel_id, discord_channel_name, discord_guild_id, created_at, updated_at FROM room_mappings WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(row.map(Self::row_to_room_mapping))
    }

    async fn list_room_mappings(&self, limit: i64, offset: i64) -> Result<Vec<RoomMapping>, DatabaseError> {
        let rows = sqlx::query(
            "SELECT id, matrix_room_id, discord_channel_id, discord_channel_name, discord_guild_id, created_at, updated_at FROM room_mappings ORDER BY id DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(rows.into_iter().map(Self::row_to_room_mapping).collect())
    }

    async fn create_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO room_mappings (matrix_room_id, discord_channel_id, discord_channel_name, discord_guild_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&mapping.matrix_room_id)
        .bind(&mapping.discord_channel_id)
        .bind(&mapping.discord_channel_name)
        .bind(&mapping.discord_guild_id)
        .bind(mapping.created_at)
        .bind(mapping.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }

    async fn update_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError> {
        sqlx::query(
            "UPDATE room_mappings SET matrix_room_id = $1, discord_channel_id = $2, discord_channel_name = $3, discord_guild_id = $4, updated_at = $5 WHERE id = $6",
        )
        .bind(&mapping.matrix_room_id)
        .bind(&mapping.discord_channel_id)
        .bind(&mapping.discord_channel_name)
        .bind(&mapping.discord_guild_id)
        .bind(mapping.updated_at)
        .bind(mapping.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }

    async fn delete_room_mapping(&self, id: i64) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM room_mappings WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }
}

pub struct PostgresUserStore {
    pool: PgPool,
}

impl PostgresUserStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl super::UserStore for PostgresUserStore {
    async fn get_user_by_discord_id(&self, discord_id: &str) -> Result<Option<UserMapping>, DatabaseError> {
        let row = sqlx::query(
            "SELECT id, matrix_user_id, discord_user_id, discord_username, discord_discriminator, discord_avatar, created_at, updated_at FROM user_mappings WHERE discord_user_id = $1",
        )
        .bind(discord_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        if let Some(row) = row {
            Ok(Some(UserMapping {
                id: row.get("id"),
                matrix_user_id: row.get("matrix_user_id"),
                discord_user_id: row.get("discord_user_id"),
                discord_username: row.get("discord_username"),
                discord_discriminator: row.get("discord_discriminator"),
                discord_avatar: row.get("discord_avatar"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }))
        } else {
            Ok(None)
        }
    }

    async fn create_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO user_mappings (matrix_user_id, discord_user_id, discord_username, discord_discriminator, discord_avatar, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&mapping.matrix_user_id)
        .bind(&mapping.discord_user_id)
        .bind(&mapping.discord_username)
        .bind(&mapping.discord_discriminator)
        .bind(&mapping.discord_avatar)
        .bind(mapping.created_at)
        .bind(mapping.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }

    async fn update_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError> {
        sqlx::query(
            "UPDATE user_mappings SET discord_username = $1, discord_discriminator = $2, discord_avatar = $3, updated_at = $4 WHERE id = $5",
        )
        .bind(&mapping.discord_username)
        .bind(&mapping.discord_discriminator)
        .bind(&mapping.discord_avatar)
        .bind(mapping.updated_at)
        .bind(mapping.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }

    async fn delete_user_mapping(&self, id: i64) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM user_mappings WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }
}
