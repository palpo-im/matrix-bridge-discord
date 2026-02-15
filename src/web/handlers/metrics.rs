use serde_json::json;
use salvo::prelude::*;

use crate::web::web_state;

#[handler]
pub async fn metrics(res: &mut Response) {
    let state = web_state();
    let uptime_seconds = state.started_at.elapsed().as_secs();

    let metrics_payload = json!({
        "bridge": {
            "status": "running",
            "uptime_seconds": uptime_seconds,
            "version": env!("CARGO_PKG_VERSION"),
        }
    });

    res.render(Json(metrics_payload));
}
