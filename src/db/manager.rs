use crate::config::DatabaseConfig;
use crate::db::{DatabaseError, RoomStore, UserStore};
use std::sync::Arc;

#[cfg(feature = "postgres")]
use crate::db::postgres::{PostgresRoomStore, PostgresUserStore};
#[cfg(feature = "postgres")]
use diesel::pg::PgConnection;
#[cfg(feature = "postgres")]
use diesel::r2d2::{self, ConnectionManager};
#[cfg(feature = "postgres")]
use diesel::RunQueryDsl;

#[cfg(feature = "postgres")]
pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;

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
            let manager = ConnectionManager::<PgConnection>::new(config.connection_string.clone());

            let builder = r2d2::Pool::builder()
                .max_size(config.max_connections.unwrap_or(10))
                .min_idle(Some(config.min_connections.unwrap_or(1)));

            let pool = builder
                .build(manager)
                .map_err(|e| DatabaseError::Connection(e.to_string()))?;

            let room_store = Arc::new(PostgresRoomStore::new(pool.clone()));
            let user_store = Arc::new(PostgresUserStore::new(pool.clone()));

            return Ok(Self {
                pool,
                room_store,
                user_store,
            });
        }

        #[cfg(not(feature = "postgres"))]
        {
            let _ = config;
            Err(DatabaseError::Connection(
                "PostgreSQL feature not enabled".to_string(),
            ))
        }
    }

    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        #[cfg(feature = "postgres")]
        {
            let pool = self.pool.clone();
            return tokio::task::spawn_blocking(move || {
                let mut conn = pool
                    .get()
                    .map_err(|e| DatabaseError::Connection(e.to_string()))?;

                let statements = [
                    r#"
                    CREATE TABLE IF NOT EXISTS user_mappings (
                        id BIGSERIAL PRIMARY KEY,
                        matrix_user_id TEXT NOT NULL UNIQUE,
                        discord_user_id TEXT NOT NULL UNIQUE,
                        discord_username TEXT NOT NULL,
                        discord_discriminator TEXT NOT NULL,
                        discord_avatar TEXT,
                        created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                        updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
                    )
                    "#,
                    r#"
                    CREATE TABLE IF NOT EXISTS room_mappings (
                        id BIGSERIAL PRIMARY KEY,
                        matrix_room_id TEXT NOT NULL UNIQUE,
                        discord_channel_id TEXT NOT NULL UNIQUE,
                        discord_channel_name TEXT NOT NULL,
                        discord_guild_id TEXT NOT NULL,
                        created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                        updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
                    )
                    "#,
                    r#"
                    CREATE TABLE IF NOT EXISTS processed_events (
                        id BIGSERIAL PRIMARY KEY,
                        event_id TEXT NOT NULL UNIQUE,
                        event_type TEXT NOT NULL,
                        source TEXT NOT NULL,
                        processed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
                    )
                    "#,
                    r#"
                    CREATE TABLE IF NOT EXISTS user_activity (
                        id BIGSERIAL PRIMARY KEY,
                        user_mapping_id BIGINT NOT NULL REFERENCES user_mappings(id) ON DELETE CASCADE,
                        activity_type TEXT NOT NULL,
                        timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                        metadata JSONB
                    )
                    "#,
                    "CREATE INDEX IF NOT EXISTS idx_user_mappings_matrix_id ON user_mappings(matrix_user_id)",
                    "CREATE INDEX IF NOT EXISTS idx_user_mappings_discord_id ON user_mappings(discord_user_id)",
                    "CREATE INDEX IF NOT EXISTS idx_room_mappings_matrix_id ON room_mappings(matrix_room_id)",
                    "CREATE INDEX IF NOT EXISTS idx_room_mappings_discord_id ON room_mappings(discord_channel_id)",
                    "CREATE INDEX IF NOT EXISTS idx_processed_events_event_id ON processed_events(event_id)",
                    "CREATE INDEX IF NOT EXISTS idx_user_activity_user_mapping ON user_activity(user_mapping_id)",
                    "CREATE INDEX IF NOT EXISTS idx_user_activity_timestamp ON user_activity(timestamp)",
                ];

                for statement in statements {
                    diesel::sql_query(statement)
                        .execute(&mut conn)
                        .map_err(|e| DatabaseError::Migration(e.to_string()))?;
                }

                Ok(())
            })
            .await
            .map_err(|e| DatabaseError::Migration(format!("migration task failed: {e}")))?;
        }

        #[cfg(not(feature = "postgres"))]
        {
            Err(DatabaseError::Migration(
                "PostgreSQL feature not enabled".to_string(),
            ))
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
