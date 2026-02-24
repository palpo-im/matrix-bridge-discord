use crate::config::{DatabaseConfig as ConfigDatabaseConfig, DbType as ConfigDbType};
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

#[cfg(feature = "sqlite")]
use crate::db::sqlite::{SqliteRoomStore, SqliteUserStore};
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection;
#[cfg(feature = "sqlite")]
use diesel::Connection;

#[derive(Clone)]
pub struct DatabaseManager {
    #[cfg(feature = "postgres")]
    postgres_pool: Option<Pool>,
    #[cfg(feature = "sqlite")]
    sqlite_path: Option<String>,
    room_store: Arc<dyn RoomStore>,
    user_store: Arc<dyn UserStore>,
    db_type: DbType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbType {
    Postgres,
    Sqlite,
}

impl From<ConfigDbType> for DbType {
    fn from(value: ConfigDbType) -> Self {
        match value {
            ConfigDbType::Postgres => DbType::Postgres,
            ConfigDbType::Sqlite => DbType::Sqlite,
        }
    }
}

impl DatabaseManager {
    pub async fn new(config: &ConfigDatabaseConfig) -> Result<Self, DatabaseError> {
        let db_type = DbType::from(config.db_type());

        match db_type {
            #[cfg(feature = "postgres")]
            DbType::Postgres => {
                let connection_string = config.connection_string();
                let max_connections = config.max_connections();
                let min_connections = config.min_connections();

                let manager = ConnectionManager::<PgConnection>::new(connection_string);

                let builder = r2d2::Pool::builder()
                    .max_size(max_connections.unwrap_or(10))
                    .min_idle(Some(min_connections.unwrap_or(1)));

                let pool = builder
                    .build(manager)
                    .map_err(|e| DatabaseError::Connection(e.to_string()))?;

                let room_store = Arc::new(PostgresRoomStore::new(pool.clone()));
                let user_store = Arc::new(PostgresUserStore::new(pool.clone()));

                return Ok(Self {
                    postgres_pool: Some(pool),
                    #[cfg(feature = "sqlite")]
                    sqlite_path: None,
                    room_store,
                    user_store,
                    db_type,
                });
            }
            #[cfg(feature = "sqlite")]
            DbType::Sqlite => {
                let path = config.sqlite_path().unwrap();

                let room_store = Arc::new(SqliteRoomStore::new());
                let user_store = Arc::new(SqliteUserStore::new());

                return Ok(Self {
                    #[cfg(feature = "postgres")]
                    postgres_pool: None,
                    sqlite_path: Some(path),
                    room_store,
                    user_store,
                    db_type,
                });
            }
            #[cfg(not(feature = "postgres"))]
            DbType::Postgres => {
                return Err(DatabaseError::Connection(
                    "PostgreSQL feature not enabled".to_string(),
                ))
            }
            #[cfg(not(feature = "sqlite"))]
            DbType::Sqlite => {
                return Err(DatabaseError::Connection(
                    "SQLite feature not enabled".to_string(),
                ))
            }
        }
    }

    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        match self.db_type {
            #[cfg(feature = "postgres")]
            DbType::Postgres => {
                let pool = self.postgres_pool.as_ref().unwrap();
                return Self::migrate_postgres(pool).await;
            }
            #[cfg(feature = "sqlite")]
            DbType::Sqlite => {
                let path = self.sqlite_path.as_ref().unwrap();
                return Self::migrate_sqlite(path).await;
            }
            #[cfg(not(feature = "postgres"))]
            DbType::Postgres => {
                return Err(DatabaseError::Migration(
                    "PostgreSQL feature not enabled".to_string(),
                ))
            }
            #[cfg(not(feature = "sqlite"))]
            DbType::Sqlite => {
                return Err(DatabaseError::Migration(
                    "SQLite feature not enabled".to_string(),
                ))
            }
        }
    }

    #[cfg(feature = "postgres")]
    async fn migrate_postgres(pool: &Pool) -> Result<(), DatabaseError> {
        let pool = pool.clone();
        tokio::task::spawn_blocking(move || {
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
        .map_err(|e| DatabaseError::Migration(format!("migration task failed: {e}")))?
    }

    #[cfg(feature = "sqlite")]
    async fn migrate_sqlite(path: &str) -> Result<(), DatabaseError> {
        let path = path.to_string();
        tokio::task::spawn_blocking(move || {
            let conn_string = format!("sqlite://{}", path);
            let mut conn = SqliteConnection::establish(&conn_string)
                .map_err(|e| DatabaseError::Connection(e.to_string()))?;

            let statements = [
                r#"
                CREATE TABLE IF NOT EXISTS user_mappings (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    matrix_user_id TEXT NOT NULL UNIQUE,
                    discord_user_id TEXT NOT NULL UNIQUE,
                    discord_username TEXT NOT NULL,
                    discord_discriminator TEXT NOT NULL,
                    discord_avatar TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                )
                "#,
                r#"
                CREATE TABLE IF NOT EXISTS room_mappings (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    matrix_room_id TEXT NOT NULL UNIQUE,
                    discord_channel_id TEXT NOT NULL UNIQUE,
                    discord_channel_name TEXT NOT NULL,
                    discord_guild_id TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                )
                "#,
                r#"
                CREATE TABLE IF NOT EXISTS processed_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    event_type TEXT NOT NULL,
                    source TEXT NOT NULL,
                    processed_at TEXT NOT NULL DEFAULT (datetime('now'))
                )
                "#,
                r#"
                CREATE TABLE IF NOT EXISTS user_activity (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    user_mapping_id INTEGER NOT NULL REFERENCES user_mappings(id) ON DELETE CASCADE,
                    activity_type TEXT NOT NULL,
                    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                    metadata TEXT
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
        .map_err(|e| DatabaseError::Migration(format!("migration task failed: {e}")))?
    }

    pub fn room_store(&self) -> Arc<dyn RoomStore> {
        self.room_store.clone()
    }

    pub fn user_store(&self) -> Arc<dyn UserStore> {
        self.user_store.clone()
    }

    #[cfg(feature = "postgres")]
    pub fn pool(&self) -> Option<&Pool> {
        self.postgres_pool.as_ref()
    }

    pub fn db_type(&self) -> DbType {
        self.db_type
    }
}
