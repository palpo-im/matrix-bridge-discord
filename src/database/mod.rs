pub use self::stores::{RoomStore, UserStore};
pub use self::models::{RoomMapping, UserMapping, ProcessedEvent};
pub use self::error::DatabaseError;
pub use self::manager::DatabaseManager;

pub mod manager;
pub mod stores;
pub mod models;
pub mod error;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "sqlite")]
pub mod sqlite;