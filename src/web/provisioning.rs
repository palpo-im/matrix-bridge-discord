use chrono::Utc;
use salvo::prelude::*;
use serde_json::json;

use crate::db::RoomMapping;
use crate::web::web_state;

fn render_error(res: &mut Response, status: StatusCode, message: &str) {
    res.status_code(status);
    res.render(Json(json!({ "error": message })));
}

#[handler]
pub async fn list_rooms(req: &mut Request, res: &mut Response) {
    let limit = req.query::<i64>("limit").unwrap_or(100).clamp(1, 1000);
    let offset = req.query::<i64>("offset").unwrap_or(0).max(0);

    match web_state()
        .db_manager
        .room_store()
        .list_room_mappings(limit, offset)
        .await
    {
        Ok(rooms) => {
            res.render(Json(json!({
                "rooms": rooms,
                "count": rooms.len(),
                "limit": limit,
                "offset": offset,
            })));
        }
        Err(err) => {
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("database error: {}", err),
            );
        }
    }
}

#[handler]
pub async fn create_bridge(req: &mut Request, res: &mut Response) {
    let matrix_room_id = match req.query::<String>("matrix_room_id") {
        Some(v) if !v.is_empty() => v,
        _ => {
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "missing matrix_room_id query parameter",
            );
            return;
        }
    };
    let discord_channel_id = match req.query::<String>("discord_channel_id") {
        Some(v) if !v.is_empty() => v,
        _ => {
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "missing discord_channel_id query parameter",
            );
            return;
        }
    };
    let discord_channel_name = req
        .query::<String>("discord_channel_name")
        .unwrap_or_else(|| discord_channel_id.clone());
    let discord_guild_id = req
        .query::<String>("discord_guild_id")
        .unwrap_or_else(|| "unknown_guild".to_string());

    let room_store = web_state().db_manager.room_store();

    match room_store.get_room_by_matrix_room(&matrix_room_id).await {
        Ok(Some(_)) => {
            render_error(res, StatusCode::CONFLICT, "matrix room is already bridged");
            return;
        }
        Ok(None) => {}
        Err(err) => {
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("database error: {}", err),
            );
            return;
        }
    }

    match room_store
        .get_room_by_discord_channel(&discord_channel_id)
        .await
    {
        Ok(Some(_)) => {
            render_error(
                res,
                StatusCode::CONFLICT,
                "discord channel is already bridged",
            );
            return;
        }
        Ok(None) => {}
        Err(err) => {
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("database error: {}", err),
            );
            return;
        }
    }

    let now = Utc::now();
    let mapping = RoomMapping {
        id: 0,
        matrix_room_id,
        discord_channel_id,
        discord_channel_name,
        discord_guild_id,
        created_at: now,
        updated_at: Utc::now(),
    };

    match room_store.create_room_mapping(&mapping).await {
        Ok(()) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(json!({
                "ok": true,
                "mapping": mapping,
            })));
        }
        Err(err) => {
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("database error: {}", err),
            );
        }
    }
}

#[handler]
pub async fn delete_bridge(req: &mut Request, res: &mut Response) {
    let id = match req.param::<i64>("id") {
        Some(v) if v > 0 => v,
        _ => {
            render_error(res, StatusCode::BAD_REQUEST, "invalid bridge id");
            return;
        }
    };

    match web_state()
        .db_manager
        .room_store()
        .delete_room_mapping(id)
        .await
    {
        Ok(()) => {
            res.render(Json(json!({ "ok": true, "id": id })));
        }
        Err(err) => {
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("database error: {}", err),
            );
        }
    }
}

#[handler]
pub async fn get_bridge_info(req: &mut Request, res: &mut Response) {
    let id = match req.param::<i64>("id") {
        Some(v) if v > 0 => v,
        _ => {
            render_error(res, StatusCode::BAD_REQUEST, "invalid bridge id");
            return;
        }
    };

    match web_state().db_manager.room_store().get_room_by_id(id).await {
        Ok(Some(mapping)) => {
            res.render(Json(json!({ "mapping": mapping })));
        }
        Ok(None) => {
            render_error(res, StatusCode::NOT_FOUND, "bridge not found");
        }
        Err(err) => {
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("database error: {}", err),
            );
        }
    }
}
