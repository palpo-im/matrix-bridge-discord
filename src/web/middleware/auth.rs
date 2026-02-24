use salvo::prelude::*;

use crate::web::handlers::{
    health::{get_status, health_check},
    metrics::metrics,
    provisioning::{create_bridge, delete_bridge, get_bridge_info, list_rooms},
};

pub fn create_router() -> Router {
    Router::new()
        .push(Router::with_path("health").get(health_check))
        .push(Router::with_path("metrics").get(metrics))
        .push(Router::with_path("status").get(get_status))
        .push(
            Router::with_path("_matrix")
                .push(Router::with_path("rooms").get(list_rooms))
                .push(Router::with_path("bridges").post(create_bridge))
                .push(
                    Router::with_path("bridges/{id}")
                        .get(get_bridge_info)
                        .delete(delete_bridge),
                ),
        )
        .push(
            Router::with_path("admin")
                .push(
                    Router::with_path("bridges")
                        .get(list_rooms)
                        .post(create_bridge),
                )
                .push(
                    Router::with_path("bridges/{id}")
                        .get(get_bridge_info)
                        .delete(delete_bridge),
                ),
        )
}
