#![forbid(unsafe_code)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_comparisons)]

use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info};

mod bridge;
mod cli;
mod config;
mod db;
mod discord;
mod matrix;
mod parsers;
mod utils;
mod web;

use config::Config;
use web::WebServer;

#[tokio::main]
async fn main() -> Result<()> {
    utils::logging::init_tracing();

    let config = Arc::new(Config::load()?);
    info!("matrix-discord bridge starting up");

    let db_manager = Arc::new(db::DatabaseManager::new(&config.database).await?);
    db_manager.migrate().await?;

    let matrix_client = Arc::new(matrix::MatrixAppservice::new(config.clone()).await?);
    let discord_client = Arc::new(discord::DiscordClient::new(config.clone()).await?);

    let bridge = Arc::new(bridge::BridgeCore::new(
        matrix_client.clone(),
        discord_client.clone(),
        db_manager.clone(),
    ));

    let web_server = WebServer::new(
        config.clone(),
        matrix_client.clone(),
        db_manager.clone(),
        bridge.clone(),
    )
    .await?;

    let web_handle = tokio::spawn(async move {
        if let Err(e) = web_server.start().await {
            error!("web server error: {}", e);
        }
    });

    let bridge_handle = tokio::spawn(async move {
        if let Err(e) = bridge.start().await {
            error!("bridge error: {}", e);
        }
    });

    tokio::select! {
        _ = web_handle => {},
        _ = bridge_handle => {},
    }

    info!("matrix-discord bridge shutting down");
    Ok(())
}
