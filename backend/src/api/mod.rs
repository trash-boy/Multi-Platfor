mod accounts;
mod discover;
mod engage;
mod posts;
mod providers;
mod works;

use crate::state::AppState;
use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, post, put},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/platforms", get(accounts::platforms))
        .route("/accounts", get(accounts::list))
        .route("/accounts/{platform}/status", get(accounts::check))
        .route("/accounts/{platform}/cookie", post(accounts::cookie_login))
        .route("/accounts/{platform}/{account}", delete(accounts::logout))
        .route("/login/{platform}", post(accounts::qr_start).get(accounts::qr_status).delete(accounts::qr_cancel))
        .route("/login/{platform}/qrcode", get(accounts::qr_image))
        .route("/login/{platform}/code", post(accounts::qr_code))
        .route("/search", get(discover::search))
        .route("/feed", get(discover::feed))
        .route("/item", get(discover::item))
        .route("/comments", get(discover::comments))
        .route("/works", get(works::works))
        .route("/media", post(posts::upload_media).layer(DefaultBodyLimit::max(1024 * 1024 * 1024 + 1024 * 1024)))
        .route("/posts", get(posts::list).post(posts::create))
        .route("/posts/{id}", get(posts::get).delete(posts::delete))
        .route("/posts/{id}/cancel", post(posts::cancel))
        .route("/posts/{id}/schedule", post(posts::reschedule))
        .route("/targets/{id}/retry", post(posts::retry_target))
        .route("/providers", get(providers::overview))
        .route("/providers/order", put(providers::set_order))
        .route("/providers/x-api", put(providers::set_x_app))
        .route("/providers/x-api/{account}", delete(providers::x_remove))
        .route("/oauth/x/start", get(providers::x_start))
        .route("/oauth/x/callback", get(providers::x_callback))
        .route("/providers/xhs-mcp", post(providers::xhs_mcp_upsert))
        .route("/providers/xhs-mcp/{account}", delete(providers::xhs_mcp_remove))
        .route("/providers/xhs-mcp/{account}/status", get(providers::xhs_mcp_status))
        .route("/providers/xhs-mcp/{account}/qrcode", get(providers::xhs_mcp_qrcode))
        .route("/providers/sau", post(providers::sau_upsert))
        .route("/providers/sau/bin", put(providers::sau_set_bin))
        .route("/providers/sau/{platform}/{account}", delete(providers::sau_remove))
        .route("/providers/sau/{platform}/{account}/check", get(providers::sau_check))
        // engagement: analytics, inbox, automation
        .route("/analytics/summary", get(engage::summary))
        .route("/analytics/items", get(engage::items))
        .route("/analytics/items/{platform}/{account}/{item_id}", get(engage::item_history))
        .route("/sync", get(engage::sync_status).post(engage::sync_now))
        .route("/inbox", get(engage::inbox))
        .route("/inbox/unread", get(engage::inbox_unread))
        .route("/inbox/read", post(engage::inbox_read))
        .route("/inbox/{id}/reply", post(engage::inbox_reply))
        .route("/inbox/{id}/messages", get(engage::inbox_messages))
        .route("/automation", get(engage::automation))
        .route("/automation/settings", put(engage::set_automation))
        .route("/automation/rules", post(engage::create_rule))
        .route("/automation/rules/{id}", put(engage::update_rule).delete(engage::delete_rule))
        .route("/automation/actions", get(engage::actions))
        .route("/automation/actions/{id}/cancel", post(engage::cancel_action))
}
