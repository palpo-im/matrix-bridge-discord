pub use self::error::DatabaseError;
pub use self::manager::DatabaseManager;
pub use self::models::{ProcessedEvent, RoomMapping, UserMapping};
pub use self::stores::{RoomStore, UserStore};

pub mod error;
pub mod manager;
pub mod models;
pub mod schema;
pub mod stores;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "sqlite")]
pub mod sqlite;
