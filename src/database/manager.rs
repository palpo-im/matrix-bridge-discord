use crate::config::DatabaseConfig;
use crate::database::{DatabaseError, RoomStore, UserStore};
use std::sync::Arc;

#[cfg(feature = "postgres")]
use crate::database::postgres::{PostgresRoomStore, PostgresUserStore};
#[cfg(feature = "postgres")]
use sqlx::{PgPool, postgres::PgPoolOptions};

#[cfg(feature = "postgres")]
pub type Pool = PgPool;

#[cfg(feature = "postgres")]
type PoolOptions = PgPoolOptions;

#[derive(Clone)]
pub struct DatabaseManager {
    #[cfg(feature = "postgres")]
    pool: Pool,
    room_store: Arc<dyn RoomStore>,
    user_store: Arc<dyn UserStore>,
}

impl DatabaseManager {
    pub async fn new(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        #[cfg(feature = "postgres")]
        {
            let pool = PoolOptions::new()
                .max_connections(config.max_connections.unwrap_or(10))
                .min_connections(config.min_connections.unwrap_or(1))
                .connect(&config.connection_string)
                .await
                .map_err(|e| DatabaseError::Connection(e.to_string()))?;

            let room_store = Arc::new(PostgresRoomStore::new(pool.clone()));
            let user_store = Arc::new(PostgresUserStore::new(pool.clone()));

            Ok(Self {
                pool,
                room_store,
                user_store,
            })
        }
        
        #[cfg(not(feature = "postgres"))]
        {
            Err(DatabaseError::Connection("PostgreSQL feature not enabled".to_string()))
        }
    }

    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        #[cfg(feature = "postgres")]
        {
            // For now, we'll run a simple table creation
            // In production, you'd use sqlx-cli for migrations
            sqlx::query(
                r#"
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
                
                CREATE TABLE IF NOT EXISTS room_mappings (
                    id SERIAL PRIMARY KEY,
                    matrix_room_id TEXT NOT NULL UNIQUE,
                    discord_channel_id TEXT NOT NULL UNIQUE,
                    discord_channel_name TEXT NOT NULL,
                    discord_guild_id TEXT NOT NULL,
                    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
                );
                
                CREATE TABLE IF NOT EXISTS processed_events (
                    id SERIAL PRIMARY KEY,
                    event_id TEXT NOT NULL UNIQUE,
                    event_type TEXT NOT NULL,
                    source TEXT NOT NULL,
                    processed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
                );
                
                CREATE INDEX IF NOT EXISTS idx_user_mappings_matrix_id ON user_mappings(matrix_user_id);
                CREATE INDEX IF NOT EXISTS idx_user_mappings_discord_id ON user_mappings(discord_user_id);
                CREATE INDEX IF NOT EXISTS idx_room_mappings_matrix_id ON room_mappings(matrix_room_id);
                CREATE INDEX IF NOT EXISTS idx_room_mappings_discord_id ON room_mappings(discord_channel_id);
                CREATE INDEX IF NOT EXISTS idx_processed_events_event_id ON processed_events(event_id);
                "#
            )
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::Migration(e.to_string()))?;

            Ok(())
        }
        
        #[cfg(not(feature = "postgres"))]
        {
            Err(DatabaseError::Migration("PostgreSQL feature not enabled".to_string()))
        }
    }

    pub fn room_store(&self) -> Arc<dyn RoomStore> {
        self.room_store.clone()
    }

    pub fn user_store(&self) -> Arc<dyn UserStore> {
        self.user_store.clone()
    }

    #[cfg(feature = "postgres")]
    pub fn pool(&self) -> &Pool {
        &self.pool
    }
}