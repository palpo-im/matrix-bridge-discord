pub use self::error::DatabaseError;
pub use self::manager::DatabaseManager;
pub use self::models::{MessageMapping, ProcessedEvent, RoomMapping, UserMapping};
pub use self::stores::{MessageStore, RoomStore, UserStore};

pub mod error;
pub mod manager;
pub mod models;
pub mod schema;
pub mod stores;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "sqlite")]
pub mod schema_sqlite;
