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
    pub ghosts: GhostConfig,
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
    #[serde(default)]
    pub homeserver_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub bot_token: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub connection_string: String,
    #[serde(default)]
    pub max_connections: Option<u32>,
    #[serde(default)]
    pub min_connections: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoomConfig {
    #[serde(default)]
    pub default_visibility: String,
    #[serde(default)]
    pub room_alias_prefix: String,
    #[serde(default)]
    pub enable_room_creation: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelConfig {
    #[serde(default)]
    pub enable_channel_creation: bool,
    #[serde(default)]
    pub channel_name_format: String,
    #[serde(default)]
    pub topic_format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhostConfig {
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
        // 首先尝试从环境变量指定的路径加载
        let config_path = std::env::var("CONFIG_PATH")
            .ok()
            .or_else(|| Some("config.yaml".to_string()))
            .unwrap();

        Self::load_from_file(&config_path)
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        // 验证必需的配置项
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

        if self.database.connection_string.is_empty() {
            return Err(ConfigError::InvalidConfig(
                "database.connection_string cannot be empty".to_string(),
            ));
        }

        // 验证端口范围
        if self.bridge.port == 0 || self.bridge.port > 65535 {
            return Err(ConfigError::InvalidConfig(
                "bridge.port must be between 1 and 65535".to_string(),
            ));
        }

        Ok(())
    }
}

// 默认值函数
fn default_port() -> u16 {
    9005
}
fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_format() -> String {
    "pretty".to_string()
}
fn default_metrics_port() -> u16 {
    9090
}
fn default_metrics_bind_address() -> String {
    "127.0.0.1".to_string()
}
