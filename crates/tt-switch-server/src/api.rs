//! HTTP API 层：把 tt-switch-core 暴露为本地 REST 接口，供 webui（浏览器）调用。
//! 仅绑定 127.0.0.1，凭据不出本机。

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rust_embed::RustEmbed;
use serde_json::{json, Value};

use tt_switch_core::modules::{account, checkin, commercial, error_log, switch, system, transfer, variant::ClientKind};

#[derive(RustEmbed)]
#[folder = "../../dist/"]
struct Assets;

pub fn router() -> Router {
    Router::new()
        .route("/api/status", get(api_status))
        .route("/api/accounts", get(api_accounts))
        .route("/api/accounts/capture", post(api_capture))
        .route("/api/accounts/rename", post(api_rename))
        .route("/api/accounts/delete", post(api_delete))
        .route("/api/switch", post(api_switch))
        .route("/api/switch/progress", get(api_switch_progress))
        .route("/api/error-logs", get(api_error_logs))
        .route("/api/system/info", get(api_system_info))
        .route("/api/system/check-permission", post(api_check_permission))
        .route("/api/system/open-privacy-settings", post(api_open_privacy_settings))
        .route("/api/system/reveal", post(api_reveal))
        .route("/api/system/delete-backup", post(api_delete_backup))
        .route("/api/transfer/export", post(api_transfer_export))
        .route("/api/transfer/import/preview", post(api_transfer_preview))
        .route("/api/transfer/import", post(api_transfer_import))
        .route("/api/commercial/stats", get(api_commercial_stats))
        .route(
            "/api/commercial/local-overview",
            get(|| async { Json(commercial::local_overview()) }),
        )
        .route("/api/checkin/status", get(api_checkin_status))
        .route("/api/checkin/config", post(api_checkin_config))
        .route("/api/checkin/run", post(api_checkin_run))
        .fallback(static_handler)
}

fn err_response(message: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message.into() }))).into_response()
}

fn parse_client(body: &Value) -> Result<ClientKind, Response> {
    ClientKind::parse(body.get("client").and_then(Value::as_str)).ok_or_else(|| {
        err_response("缺少有效 client 参数（ide_cn / solo_cn / ide_intl）")
    })
}

/// GET /api/status —— 三客户端状态 + 账号数。
async fn api_status() -> Json<Value> {
    let clients: Vec<Value> = ClientKind::ALL.iter().map(|k| switch::client_status(*k)).collect();
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "clients": clients,
        "accountCount": account::load_accounts().len(),
    }))
}

/// GET /api/accounts —— 账号列表（元数据，不含凭据）。
async fn api_accounts() -> Json<Value> {
    let metas: Vec<Value> = account::load_accounts()
        .iter()
        .map(account::account_meta)
        .collect();
    Json(json!({ "accounts": metas }))
}

/// POST /api/accounts/capture {client} —— 采集当前登录态为账号。
async fn api_capture(Json(body): Json<Value>) -> Response {
    let kind = match parse_client(&body) {
        Ok(k) => k,
        Err(r) => return r,
    };
    match switch::capture_current(kind) {
        Ok(record) => Json(json!({ "ok": true, "account": account::account_meta(&record) }))
            .into_response(),
        Err(e) => {
            error_log::log_error("capture", &e);
            err_response(e)
        }
    }
}

/// POST /api/accounts/rename {id, nickname}
async fn api_rename(Json(body): Json<Value>) -> Response {
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let nickname = body.get("nickname").and_then(Value::as_str).unwrap_or("");
    match account::rename_account(id, nickname) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err_response(e),
    }
}

/// POST /api/accounts/delete {id}
async fn api_delete(Json(body): Json<Value>) -> Response {
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    match account::delete_account(id) {
        Ok(()) => {
            tt_switch_core::modules::snapshot::remove_snapshot(id);
            Json(json!({ "ok": true })).into_response()
        }
        Err(e) => err_response(e),
    }
}

/// POST /api/switch {id, restart?}
async fn api_switch(Json(body): Json<Value>) -> Response {
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() {
        return err_response("缺少 id");
    }
    let restart = body.get("restart").and_then(Value::as_bool).unwrap_or(true);
    match switch::switch_account(id, restart, None) {
        Ok(report) => Json(report).into_response(),
        Err(e) => {
            error_log::log_error("switch", &e);
            err_response(e)
        }
    }
}

/// GET /api/switch/progress
async fn api_switch_progress() -> Json<Value> {
    Json(switch::progress_snapshot())
}

/// GET /api/error-logs
async fn api_error_logs() -> Json<Value> {
    Json(json!({
        "path": error_log::error_log_path(),
        "logs": error_log::load_error_logs(),
    }))
}

/// GET /api/system/info —— 存储路径 + 备份清单。
async fn api_system_info() -> Json<Value> {
    let mut info = system::storage_paths();
    if let Some(obj) = info.as_object_mut() {
        obj.insert("backups".to_string(), json!(system::list_backups()));
    }
    Json(info)
}

/// POST /api/system/check-permission {client}
async fn api_check_permission(Json(body): Json<Value>) -> Response {
    let kind = match parse_client(&body) {
        Ok(k) => k,
        Err(r) => return r,
    };
    Json(system::check_data_dir_permission(kind)).into_response()
}

/// POST /api/system/open-privacy-settings {target?: "all_files"|"app_management"}
async fn api_open_privacy_settings(Json(body): Json<Value>) -> Response {
    let target = body.get("target").and_then(Value::as_str).unwrap_or("all_files");
    match system::open_privacy_settings(target) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err_response(e),
    }
}

/// POST /api/system/reveal {target, client?}
/// target: store | backups | errorLog | clientData | clientApp
async fn api_reveal(Json(body): Json<Value>) -> Response {
    let target = body.get("target").and_then(Value::as_str).unwrap_or("store");
    let result = match target {
        "store" => system::reveal_path(&tt_switch_core::modules::config::store_dir()),
        "backups" => {
            let dir = tt_switch_core::modules::config::backup_dir();
            let _ = std::fs::create_dir_all(&dir);
            system::reveal_path(&dir)
        }
        "errorLog" => system::reveal_path(&tt_switch_core::modules::config::error_log_file()),
        "clientData" | "clientApp" => match parse_client(&body) {
            Ok(kind) => {
                if target == "clientApp" {
                    system::reveal_client_app(kind)
                } else {
                    system::reveal_path(&kind.data_root())
                }
            }
            Err(r) => return r,
        },
        _ => return err_response("未知 target"),
    };
    match result {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err_response(e),
    }
}

/// POST /api/system/delete-backup {name} —— 仅允许删除备份目录内的 `<client>-*` 目录。
async fn api_delete_backup(Json(body): Json<Value>) -> Response {
    let Some(name) = body.get("name").and_then(Value::as_str) else {
        return err_response("缺少 name");
    };
    // 只接受备份目录名形态：client 前缀 + 无路径分隔符/逃逸。
    let valid_prefix = ClientKind::ALL
        .iter()
        .any(|k| name.starts_with(&format!("{}-", k.as_str())));
    if !valid_prefix || name.contains('/') || name.contains("..") {
        return err_response("非法备份名");
    }
    let path = tt_switch_core::modules::config::backup_dir().join(name);
    if !path.is_dir() {
        return err_response("备份不存在");
    }
    match std::fs::remove_dir_all(&path) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err_response(format!("删除失败: {e}")),
    }
}

/// POST /api/transfer/export {ids?, path} —— 导出账号迁移包（webui 需提供绝对路径）。
async fn api_transfer_export(Json(body): Json<Value>) -> Response {
    let ids: Vec<String> = body
        .get("ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let Some(path) = body.get("path").and_then(Value::as_str) else {
        return err_response("缺少导出路径 path");
    };
    match transfer::export_accounts(&ids, path) {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(e),
    }
}

/// POST /api/transfer/import/preview {path}
async fn api_transfer_preview(Json(body): Json<Value>) -> Response {
    let Some(path) = body.get("path").and_then(Value::as_str) else {
        return err_response("缺少迁移包路径 path");
    };
    match transfer::preview_import(path) {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(e),
    }
}

/// POST /api/transfer/import {path, indexes}
async fn api_transfer_import(Json(body): Json<Value>) -> Response {
    let Some(path) = body.get("path").and_then(Value::as_str) else {
        return err_response("缺少迁移包路径 path");
    };
    let indexes: Vec<usize> = body
        .get("indexes")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_u64).map(|n| n as usize).collect())
        .unwrap_or_default();
    match transfer::import_accounts(path, &indexes) {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(e),
    }
}

/// GET /api/commercial/stats?accountId=xxx —— 积分/Token 商业数据。
/// accountId 缺省时取当前活动 IDE 账号。
async fn api_commercial_stats(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> Json<Value> {
    let account_id = params.get("accountId").map(String::as_str);
    Json(commercial::get_stats(account_id).await)
}

/// GET /api/checkin/status —— 签到配置 + 每账号最近结果。
async fn api_checkin_status() -> Json<Value> {
    Json(checkin::status_summary())
}

/// POST /api/checkin/config {enabled?, time?} —— 更新自动签到开关 / 时间。
async fn api_checkin_config(Json(body): Json<Value>) -> Response {
    let enabled = body.get("enabled").and_then(Value::as_bool);
    let time = body.get("time").and_then(Value::as_str);
    match checkin::set_config(enabled, time) {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(e),
    }
}

/// POST /api/checkin/run —— 立即手动触发全账号签到（同日幂等仍生效）。
async fn api_checkin_run() -> Json<Value> {
    Json(checkin::run_all(true).await)
}

/// 静态资源（vite build 产物内嵌）；SPA 回落 index.html。
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess_from_path(path);
            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(content.data.into_owned()))
                .unwrap()
                .into_response()
        }
        None => match Assets::get("index.html") {
            Some(content) => Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(content.data.into_owned()))
                .unwrap()
                .into_response(),
            None => (StatusCode::NOT_FOUND, "webui 未构建，请先 npm run build").into_response(),
        },
    }
}

fn mime_guess_from_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
