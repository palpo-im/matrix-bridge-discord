use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, info, warn};

use super::{MatrixAppservice, MatrixEvent};
use crate::bridge::BridgeCore;

const AGE_LIMIT_MS: i64 = 900000;

#[async_trait]
pub trait MatrixEventHandler: Send + Sync {
    async fn handle_room_message(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_room_member(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_presence(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_room_encryption(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_room_name(&self, event: &MatrixEvent) -> Result<()>;
    async fn handle_room_topic(&self, event: &MatrixEvent) -> Result<()>;
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

    async fn handle_room_name(&self, event: &MatrixEvent) -> Result<()> {
        if let Some(bridge) = &self.bridge {
            bridge.handle_matrix_room_name(event).await?;
        } else {
            debug!("matrix room name received without bridge binding");
        }
        Ok(())
    }

    async fn handle_room_topic(&self, event: &MatrixEvent) -> Result<()> {
        if let Some(bridge) = &self.bridge {
            bridge.handle_matrix_room_topic(event).await?;
        } else {
            debug!("matrix room topic received without bridge binding");
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

    fn check_event_age(event: &MatrixEvent) -> bool {
        if let Some(ts_str) = &event.timestamp {
            if let Ok(ts) = ts_str.parse::<i64>() {
                let now = chrono::Utc::now().timestamp_millis();
                if ts > now {
                    debug!("event timestamp is in the future, allowing event_id={:?}", event.event_id);
                    return true;
                }
                let age = now - ts;
                if age > AGE_LIMIT_MS {
                    info!(
                        "skipping event due to age {}ms > {}ms event_id={:?} room_id={} type={}",
                        age, AGE_LIMIT_MS, event.event_id, event.room_id, event.event_type
                    );
                    return false;
                }
            }
        }
        true
    }

    pub async fn process_event(&self, event: MatrixEvent) -> Result<()> {
        if !Self::check_event_age(&event) {
            return Ok(());
        }

        match event.event_type.as_str() {
            "m.room.message" => self.event_handler.handle_room_message(&event).await?,
            "m.room.member" => self.event_handler.handle_room_member(&event).await?,
            "m.presence" => self.event_handler.handle_presence(&event).await?,
            "m.room.encryption" => self.event_handler.handle_room_encryption(&event).await?,
            "m.room.name" => self.event_handler.handle_room_name(&event).await?,
            "m.room.topic" => self.event_handler.handle_room_topic(&event).await?,
            other => warn!("unhandled matrix event type: {}", other),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(ts: Option<&str>) -> MatrixEvent {
        MatrixEvent {
            event_id: Some("$test".to_string()),
            event_type: "m.room.message".to_string(),
            room_id: "!room:example.org".to_string(),
            sender: "@user:example.org".to_string(),
            state_key: None,
            content: None,
            timestamp: ts.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn check_event_age_allows_recent_events() {
        let now = chrono::Utc::now().timestamp_millis();
        let event = make_event(Some(&now.to_string()));
        assert!(MatrixEventProcessor::check_event_age(&event));
    }

    #[test]
    fn check_event_age_rejects_old_events() {
        let old_ts = chrono::Utc::now().timestamp_millis() - 1_000_000;
        let event = make_event(Some(&old_ts.to_string()));
        assert!(!MatrixEventProcessor::check_event_age(&event));
    }

    #[test]
    fn check_event_age_allows_events_without_timestamp() {
        let event = make_event(None);
        assert!(MatrixEventProcessor::check_event_age(&event));
    }

    #[test]
    fn check_event_age_allows_future_events() {
        let future_ts = chrono::Utc::now().timestamp_millis() + 60_000;
        let event = make_event(Some(&future_ts.to_string()));
        assert!(MatrixEventProcessor::check_event_age(&event));
    }
}
