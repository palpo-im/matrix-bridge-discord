//! SQLite backend placeholder.
//!
//! The current runtime path is PostgreSQL-first. This module exists so tooling
//! (`rustfmt`, `cargo check --all-features`) can resolve `cfg(feature = "sqlite")`
//! without missing-file errors while SQLite parity is implemented.
