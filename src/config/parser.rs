use super::ConfigError;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub bridge: BridgeConfig,
    pub auth: AuthConfig,
    pub logging: LoggingConfig,
    pub database: DatabaseConfig,
    pub room: RoomConfig,
    pub channel: ChannelConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    pub ghosts: GhostsConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BridgeConfig {
    pub domain: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    pub bridge_id: String,
    #[serde(alias = "as_token")]
    pub appservice_token: String,
    #[serde(alias = "hs_token")]
    pub homeserver_token: String,
    #[serde(default)]
    pub homeserver_url: String,
    #[serde(default = "default_presence_interval")]
    pub presence_interval: u64,
    #[serde(default)]
    pub disable_presence: bool,
    #[serde(default)]
    pub disable_typing_notifications: bool,
    #[serde(default)]
    pub disable_discord_mentions: bool,
    #[serde(default)]
    pub disable_deletion_forwarding: bool,
    #[serde(default)]
    pub enable_self_service_bridging: bool,
    #[serde(default)]
    pub disable_portal_bridging: bool,
    #[serde(default)]
    pub disable_read_receipts: bool,
    #[serde(default)]
    pub disable_everyone_mention: bool,
    #[serde(default)]
    pub disable_here_mention: bool,
    #[serde(default)]
    pub disable_join_leave_notifications: bool,
    #[serde(default)]
    pub disable_invite_notifications: bool,
    #[serde(default)]
    pub disable_room_topic_notifications: bool,
    #[serde(default)]
    pub determine_code_language: bool,
    #[serde(default)]
    pub user_limit: Option<u32>,
    #[serde(default)]
    pub admin_mxid: Option<String>,
    #[serde(default = "default_invalid_token_message")]
    pub invalid_token_message: String,
    #[serde(default)]
    pub user_activity: Option<UserActivityConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserActivityConfig {
    #[serde(default)]
    pub min_user_active_days: u64,
    #[serde(default)]
    pub inactive_after_days: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub bot_token: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default = "default_use_privileged_intents")]
    pub use_privileged_intents: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(alias = "console", default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_line_date_format")]
    pub line_date_format: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub files: Vec<LoggingFileConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingFileConfig {
    pub file: String,
    #[serde(default = "default_log_file_level")]
    pub level: String,
    #[serde(default = "default_log_max_files")]
    pub max_files: String,
    #[serde(default = "default_log_max_size")]
    pub max_size: String,
    #[serde(default = "default_log_date_pattern")]
    pub date_pattern: String,
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub conn_string: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub user_store_path: Option<String>,
    #[serde(default)]
    pub room_store_path: Option<String>,
    #[serde(default)]
    pub max_connections: Option<u32>,
    #[serde(default)]
    pub min_connections: Option<u32>,
}

impl DatabaseConfig {
    pub fn db_type(&self) -> DbType {
        let url = self.connection_string();
        if url.starts_with("sqlite://") {
            DbType::Sqlite
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            DbType::Postgres
        } else {
            DbType::Postgres
        }
    }

    pub fn connection_string(&self) -> String {
        if let Some(ref url) = self.url {
            url.clone()
        } else if let Some(ref conn) = self.conn_string {
            conn.clone()
        } else if let Some(ref file) = self.filename {
            format!("sqlite://{}", file)
        } else {
            String::new()
        }
    }

    pub fn sqlite_path(&self) -> Option<String> {
        if let DbType::Sqlite = self.db_type() {
            let url = self.connection_string();
            Some(url.strip_prefix("sqlite://").unwrap_or(&url).to_string())
        } else {
            None
        }
    }

    pub fn max_connections(&self) -> Option<u32> {
        match self.db_type() {
            DbType::Postgres => self.max_connections,
            DbType::Sqlite => Some(1),
        }
    }

    pub fn min_connections(&self) -> Option<u32> {
        match self.db_type() {
            DbType::Postgres => self.min_connections,
            DbType::Sqlite => Some(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbType {
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoomConfig {
    #[serde(default)]
    pub default_visibility: String,
    #[serde(default)]
    pub room_alias_prefix: String,
    #[serde(default)]
    pub enable_room_creation: bool,
    #[serde(default = "default_kick_for")]
    pub kick_for: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelConfig {
    #[serde(default = "default_channel_name_pattern")]
    pub name_pattern: String,
    #[serde(default)]
    pub enable_channel_creation: bool,
    #[serde(default)]
    pub channel_name_format: String,
    #[serde(default)]
    pub topic_format: String,
    #[serde(default)]
    pub delete_options: ChannelDeleteOptionsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelDeleteOptionsConfig {
    #[serde(default)]
    pub name_prefix: Option<String>,
    #[serde(default)]
    pub topic_prefix: Option<String>,
    #[serde(default)]
    pub disable_messaging: bool,
    #[serde(default = "default_unset_room_alias")]
    pub unset_room_alias: bool,
    #[serde(default = "default_unlist_from_directory")]
    pub unlist_from_directory: bool,
    #[serde(default = "default_set_invite_only")]
    pub set_invite_only: bool,
    #[serde(default = "default_ghosts_leave")]
    pub ghosts_leave: bool,
}

impl Default for ChannelDeleteOptionsConfig {
    fn default() -> Self {
        Self {
            name_prefix: None,
            topic_prefix: None,
            disable_messaging: false,
            unset_room_alias: true,
            unlist_from_directory: true,
            set_invite_only: true,
            ghosts_leave: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LimitsConfig {
    #[serde(default = "default_room_ghost_join_delay")]
    pub room_ghost_join_delay: u64,
    #[serde(default = "default_discord_send_delay")]
    pub discord_send_delay: u64,
    #[serde(default = "default_room_count")]
    pub room_count: i32,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            room_ghost_join_delay: 6000,
            discord_send_delay: 1500,
            room_count: -1,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhostsConfig {
    #[serde(default = "default_nick_pattern")]
    pub nick_pattern: String,
    #[serde(default = "default_username_pattern")]
    pub username_pattern: String,
    #[serde(default)]
    pub username_template: String,
    #[serde(default)]
    pub displayname_template: String,
    #[serde(default)]
    pub avatar_url_template: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MetricsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_metrics_port")]
    pub port: u16,
    #[serde(default = "default_metrics_bind_address")]
    pub bind_address: String,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = std::env::var("CONFIG_PATH")
            .ok()
            .or_else(|| Some("config.yaml".to_string()))
            .unwrap();

        Self::load_from_file(&config_path)
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Config = serde_yaml::from_str(&content)?;
        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.bridge.domain.is_empty() {
            return Err(ConfigError::InvalidConfig(
                "bridge.domain cannot be empty".to_string(),
            ));
        }

        if self.auth.bot_token.is_empty() {
            return Err(ConfigError::InvalidConfig(
                "auth.bot_token cannot be empty".to_string(),
            ));
        }

        if self.database.connection_string().is_empty() {
            return Err(ConfigError::InvalidConfig(
                "database connection string cannot be empty".to_string(),
            ));
        }

        if self.bridge.port == 0 {
            return Err(ConfigError::InvalidConfig(
                "bridge.port must be between 1 and 65535".to_string(),
            ));
        }

        Ok(())
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(value) = std::env::var("APPSERVICE_DISCORD_AUTH_BOT_TOKEN") {
            self.auth.bot_token = value;
        }
        if let Ok(value) = std::env::var("APPSERVICE_DISCORD_AUTH_CLIENT_ID") {
            self.auth.client_id = Some(value);
        }
        if let Ok(value) = std::env::var("APPSERVICE_DISCORD_AUTH_CLIENT_SECRET") {
            self.auth.client_secret = Some(value);
        }
    }
}

fn default_port() -> u16 {
    9005
}

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_presence_interval() -> u64 {
    500
}

fn default_invalid_token_message() -> String {
    "Your Discord bot token seems to be invalid, and the bridge cannot function. Please update it in your bridge settings and restart the bridge".to_string()
}

fn default_use_privileged_intents() -> bool {
    false
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_line_date_format() -> String {
    "MMM-D HH:mm:ss.SSS".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

fn default_log_file_level() -> String {
    "info".to_string()
}

fn default_log_max_files() -> String {
    "14d".to_string()
}

fn default_log_max_size() -> String {
    "50m".to_string()
}

fn default_log_date_pattern() -> String {
    "YYYY-MM-DD".to_string()
}

fn default_kick_for() -> u64 {
    30000
}

fn default_channel_name_pattern() -> String {
    "[Discord] :guild :name".to_string()
}

fn default_unset_room_alias() -> bool {
    true
}

fn default_unlist_from_directory() -> bool {
    true
}

fn default_set_invite_only() -> bool {
    true
}

fn default_ghosts_leave() -> bool {
    true
}

fn default_room_ghost_join_delay() -> u64 {
    6000
}

fn default_discord_send_delay() -> u64 {
    1500
}

fn default_room_count() -> i32 {
    -1
}

fn default_nick_pattern() -> String {
    ":nick".to_string()
}

fn default_username_pattern() -> String {
    ":username#:tag".to_string()
}

fn default_metrics_port() -> u16 {
    9001
}

fn default_metrics_bind_address() -> String {
    "127.0.0.1".to_string()
}
