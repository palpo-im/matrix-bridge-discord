use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use once_cell::sync::OnceCell;
use salvo::prelude::*;
use tracing::info;

use crate::bridge::BridgeCore;
use crate::config::Config;
use crate::db::DatabaseManager;
use crate::matrix::MatrixAppservice;

pub mod handlers;
pub mod middleware;

use self::middleware::auth::create_router;

#[derive(Clone)]
pub struct WebState {
    pub db_manager: Arc<DatabaseManager>,
    pub matrix_client: Arc<MatrixAppservice>,
    pub bridge: Arc<BridgeCore>,
    pub started_at: Instant,
}

static WEB_STATE: OnceCell<WebState> = OnceCell::new();

pub fn web_state() -> &'static WebState {
    WEB_STATE
        .get()
        .expect("web state is not initialized before handler execution")
}

#[derive(Clone)]
pub struct WebServer {
    config: Arc<Config>,
}

impl WebServer {
    pub async fn new(
        config: Arc<Config>,
        matrix_client: Arc<MatrixAppservice>,
        db_manager: Arc<DatabaseManager>,
        bridge: Arc<BridgeCore>,
    ) -> Result<Self> {
        let _ = WEB_STATE.set(WebState {
            db_manager,
            matrix_client,
            bridge,
            started_at: Instant::now(),
        });

        Ok(Self { config })
    }

    pub async fn start(&self) -> Result<()> {
        let bind_addr = format!(
            "{}:{}",
            self.config.bridge.bind_address, self.config.bridge.port
        );
        info!("Starting web server on {}", bind_addr);

        let acceptor = TcpListener::new(bind_addr).bind().await;
        Server::new(acceptor).serve(create_router()).await;

        Ok(())
    }
}
