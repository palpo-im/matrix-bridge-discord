use async_trait::async_trait;

use super::DatabaseError;
use super::models::{RoomMapping, UserMapping};

#[async_trait]
pub trait RoomStore: Send + Sync {
    async fn get_room_by_discord_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<RoomMapping>, DatabaseError>;
    async fn get_room_by_matrix_room(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomMapping>, DatabaseError>;
    async fn get_room_by_id(&self, id: i64) -> Result<Option<RoomMapping>, DatabaseError>;
    async fn count_rooms(&self) -> Result<i64, DatabaseError>;
    async fn list_room_mappings(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RoomMapping>, DatabaseError>;
    async fn create_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError>;
    async fn update_room_mapping(&self, mapping: &RoomMapping) -> Result<(), DatabaseError>;
    async fn delete_room_mapping(&self, id: i64) -> Result<(), DatabaseError>;
}

#[async_trait]
pub trait UserStore: Send + Sync {
    async fn get_user_by_discord_id(
        &self,
        discord_id: &str,
    ) -> Result<Option<UserMapping>, DatabaseError>;
    async fn create_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError>;
    async fn update_user_mapping(&self, mapping: &UserMapping) -> Result<(), DatabaseError>;
    async fn delete_user_mapping(&self, id: i64) -> Result<(), DatabaseError>;
}
