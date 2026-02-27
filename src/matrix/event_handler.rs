use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, warn};

use super::{MatrixAppservice, MatrixEvent};
use crate::bridge::BridgeCore;

#[async_trait]
pub trait MatrixEventHandler: Send + Sync {
    async fn handle_room_message(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_room_member(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_presence(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_room_encryption(&self, event: &MatrixEvent) -> Result<()>;
}

pub struct MatrixEventHandlerImpl {
    _appservice: Arc<MatrixAppservice>,
    bridge: Option<Arc<BridgeCore>>,
}

impl MatrixEventHandlerImpl {
    pub fn new(appservice: Arc<MatrixAppservice>) -> Self {
        Self {
            _appservice: appservice,
            bridge: None,
        }
    }

    pub fn set_bridge(&mut self, bridge: Arc<BridgeCore>) {
        self.bridge = Some(bridge);
    }
}

#[async_trait]
impl MatrixEventHandler for MatrixEventHandlerImpl {
    async fn handle_room_message(&self, event: &MatrixEvent) -> Result<()> {
        if let Some(bridge) = &self.bridge {
            bridge.handle_matrix_message(event).await?;
        } else {
            debug!("matrix message received without bridge binding");
        }
        Ok(())
    }

    async fn handle_room_member(&self, event: &MatrixEvent) -> Result<()> {
        if let Some(bridge) = &self.bridge {
            bridge.handle_matrix_member(event).await?;
        } else {
            debug!("matrix member received without bridge binding");
        }
        Ok(())
    }

    async fn handle_presence(&self, _event: &MatrixEvent) -> Result<()> {
        Ok(())
    }

    async fn handle_room_encryption(&self, event: &MatrixEvent) -> Result<()> {
        if let Some(bridge) = &self.bridge {
            bridge.handle_matrix_encryption(event).await?;
        } else {
            debug!("matrix encryption received without bridge binding");
        }
        Ok(())
    }
}

pub struct MatrixEventProcessor {
    event_handler: Arc<dyn MatrixEventHandler>,
}

impl MatrixEventProcessor {
    pub fn new(event_handler: Arc<dyn MatrixEventHandler>) -> Self {
        Self { event_handler }
    }

    pub async fn process_event(&self, event: MatrixEvent) -> Result<()> {
        match event.event_type.as_str() {
            "m.room.message" => self.event_handler.handle_room_message(&event).await?,
            "m.room.member" => self.event_handler.handle_room_member(&event).await?,
            "m.presence" => self.event_handler.handle_presence(&event).await?,
            "m.room.encryption" => self.event_handler.handle_room_encryption(&event).await?,
            other => warn!("unhandled matrix event type: {}", other),
        }
        Ok(())
    }
}
