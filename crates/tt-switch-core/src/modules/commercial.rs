//! 商业数据（积分余额 / Token 用量）采集。
//!
//! 数据源优先级：
//! 1. 云端 commercial API（`trae-api-cn.mchost.guru/api/v1/commercial/*`），
//!    用账号快照中的 agent token 调用；鉴权失败时返回错误码，前端据此降级。
//! 2. IDE 本地缓存 `iCubeServerData://icube.cloudide`（明文 JSON，含权益/活动/支付状态）。
//! 3. state.vscdb 中的会话统计（会话数、最近活跃时间），作为 Token 页的本地兜底。
//!
//! 合规：token 仅在内存中用于请求头，绝不落日志、不落盘；日志只输出长度与过期状态。

use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use crate::modules::account;
use crate::modules::config::now_ms;
use crate::modules::snapshot::{self, AGENT_TOKEN_SNAPSHOT_NAME};
use crate::modules::variant::ClientKind;

// ---------------------------------------------------------------------------
// 云端 API
// ---------------------------------------------------------------------------

const AGENT_API_HOST: &str = "https://trae-api-cn.mchost.guru";

/// 单次 HTTP 超时（含连接）。
const HTTP_TIMEOUT_SECS: u64 = 12;

fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent(concat!("tt-switch/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// 调用 commercial 接口；token 仅放入请求头，不打印。
async fn post_commercial(token: &str, path: &str, body: &Value) -> Result<Value, String> {
    let client = http_client().map_err(|e| format!("http client init failed: {e}"))?;
    let url = format!("{AGENT_API_HOST}{path}");
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("x-ide-token", token)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("http {status}"));
    }
    serde_json::from_str::<Value>(&text).map_err(|e| format!("parse json failed: {e}"))
}

/// 积分余额 / 到期。
pub async fn fetch_balance(token: &str) -> Value {
    match post_commercial(token, "/api/v1/commercial/check_estimated_balance", &json!({})).await {
        Ok(v) => json!({ "ok": true, "data": v }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

/// 会话 Token 用量。
pub async fn fetch_session_usage(token: &str) -> Value {
    match post_commercial(token, "/api/v1/commercial/get_session_usage", &json!({})).await {
        Ok(v) => json!({ "ok": true, "data": v }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

/// 用户活动（签到等）。
pub async fn fetch_user_activity(token: &str) -> Value {
    match post_commercial(token, "/api/v1/commercial/get_user_activity", &json!({})).await {
        Ok(v) => json!({ "ok": true, "data": v }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

// ---------------------------------------------------------------------------
// 本地缓存（iCubeServerData）
// ---------------------------------------------------------------------------

/// storage.json 中缓存的商业数据键名（明文 JSON，非加密）。
const SERVER_DATA_KEY: &str = "iCubeServerData://icube.cloudide";

/// 判断当前活动客户端：数据根 storage.json 存在且缓存同步时间最新的档位。
///
/// IDE 与 SOLO 各自维护独立的商业缓存，最近同步者即为「当前登录使用中」。
pub fn active_client() -> Option<ClientKind> {
    let mut best: Option<(ClientKind, i64)> = None;
    for kind in ClientKind::ALL {
        let Some(data) = read_cached_server_data(kind) else {
            continue;
        };
        let sync = data
            .get("serverTimeInfo")
            .and_then(|t| t.get("lastSyncTime"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if sync > 0 && best.map_or(true, |(_, b)| sync > b) {
            best = Some((kind, sync));
        }
    }
    best.map(|(kind, _)| kind)
}

/// 从客户端数据根的 storage.json 读取缓存的商业数据。
///
/// 仅当该客户端为「当前活动」账号时数据根才有最新缓存；快照账号无此文件。
pub fn read_cached_server_data(kind: ClientKind) -> Option<Value> {
    let storage_json = kind.data_root().join("User/globalStorage/storage.json");
    let text = std::fs::read_to_string(&storage_json).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    // storage.json 中该值以字符串形式存储，需二次解析。
    match v.get(SERVER_DATA_KEY) {
        Some(Value::String(s)) => serde_json::from_str(s).ok(),
        Some(other) => Some(other.clone()),
        None => None,
    }
}

// ---------------------------------------------------------------------------
// 会话统计（state.vscdb）
// ---------------------------------------------------------------------------

/// 从 state.vscdb 统计某 uid 的会话数与最近活跃草稿时间。
///
/// 只读非敏感元数据（键名、时间戳），不读草稿正文。
pub fn read_session_stats(db_path: &Path, uid: &str) -> Value {
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return json!({ "error": "cannot open state.vscdb" });
    };
    let prefix = format!("{uid}:draft:session:");
    let mut count = 0i64;
    let mut last_ts: i64 = 0;
    let sql = format!(
        "SELECT key FROM ItemTable WHERE key LIKE '{}%'",
        prefix.replace('\'', "''")
    );
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for key in rows.flatten() {
                count += 1;
                // 草稿文件名含创建时间戳（16 进制），取最大作为最近活跃近似值。
                if let Some(ts) = extract_session_ts(&key) {
                    if ts > last_ts {
                        last_ts = ts;
                    }
                }
            }
        }
    }
    json!({
        "sessionCount": count,
        "lastActiveMs": last_ts,
    })
}

/// 从 draft 键名 `{uid}:draft:session:{ts-hex}:code` 提取时间戳。
fn extract_session_ts(key: &str) -> Option<i64> {
    let rest = key.split(":draft:session:").nth(1)?;
    let hex = rest.split(':').next()?;
    i64::from_str_radix(hex, 16).ok()
}

// ---------------------------------------------------------------------------
// 账号 token 读取（仅内存，不落日志）
// ---------------------------------------------------------------------------

/// 读取账号快照中的 agent token（不打印内容）。
fn read_account_token(account_id: &str) -> Option<String> {
    let path = snapshot::snapshot_dir(account_id).join(AGENT_TOKEN_SNAPSHOT_NAME);
    std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

// ---------------------------------------------------------------------------
// 聚合入口
// ---------------------------------------------------------------------------

/// 取某账号的完整商业数据：云端 API + 本地缓存 + 会话统计。
///
/// `account_id` 为空时取当前活动客户端（ide_cn）的实时数据。
pub async fn get_stats(account_id: Option<&str>) -> Value {
    let mut result = json!({
        "fetchedAt": now_ms(),
        "balance": null,
        "sessionUsage": null,
        "userActivity": null,
        "cachedServerData": null,
        "sessionStats": null,
    });

    // 1. 当前活动客户端的本地缓存（按各数据根缓存同步时间判断）。
    let active_kind = active_client().unwrap_or(ClientKind::IdeCn);
    if let Some(cached) = read_cached_server_data(active_kind) {
        result["cachedServerData"] = cached;
    }

    // 2. 会话统计（state.vscdb）
    if let Some(id) = account_id {
        if let Some(acc) = account::find_account(id) {
            if let Some(uid) = account::get_str(&acc, "uid") {
                let db_path = snapshot::snapshot_dir(id).join("User/globalStorage/state.vscdb");
                if db_path.exists() {
                    result["sessionStats"] = read_session_stats(&db_path, &uid);
                }
            }
        }
    } else {
        // 活动账号：直接读数据根
        let db_path = active_kind.data_root().join("User/globalStorage/state.vscdb");
        // 从 agent token 解析 uid
        if let Some(ident) = snapshot::parse_jwt_identity(&active_kind.agent_token_path()) {
            result["sessionStats"] = read_session_stats(&db_path, &ident.user_id);
        }
    }

    // 3. 云端 API（用快照 token 或活动 token）
    let token = if let Some(id) = account_id {
        read_account_token(id)
    } else {
        std::fs::read_to_string(&active_kind.agent_token_path())
            .ok()
            .map(|s| s.trim().to_string())
    };

    if let Some(tok) = token {
        // 顺序调用三个接口（core 不引入 tokio 宏，保持依赖最小）。
        result["balance"] = fetch_balance(&tok).await;
        result["sessionUsage"] = fetch_session_usage(&tok).await;
        result["userActivity"] = fetch_user_activity(&tok).await;
    }

    result
}

/// 本地全账号概览（零网络请求）：所有账号的会话统计 + 活动客户端商业缓存。
///
/// Token 统计页的跨账号对比图与悬浮窗都用它，毫秒级返回。
pub fn local_overview() -> Value {
    let mut accounts_json = Vec::new();
    for acc in account::load_accounts() {
        let Some(id) = account::get_str(&acc, "id") else {
            continue;
        };
        let db_path = snapshot::snapshot_dir(&id).join("User/globalStorage/state.vscdb");
        let stats = if db_path.exists() {
            let uid = account::get_str(&acc, "uid").unwrap_or_default();
            read_session_stats(&db_path, &uid)
        } else {
            json!({ "sessionCount": 0, "lastActiveMs": 0 })
        };
        accounts_json.push(json!({
            "id": id,
            "displayName": account::account_display_name(&acc),
            "client": account::get_str(&acc, "client"),
            "sessionCount": stats.get("sessionCount").cloned().unwrap_or(json!(0)),
            "lastActiveMs": stats.get("lastActiveMs").cloned().unwrap_or(json!(0)),
        }));
    }

    let active = active_client();
    json!({
        "fetchedAt": now_ms(),
        "activeClient": active.map(|k| json!(k.as_str())).unwrap_or(Value::Null),
        "cachedServerData": active.and_then(read_cached_server_data),
        "accounts": accounts_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::variant::ClientKind;

    #[test]
    fn extract_session_ts_parses_hex() {
        let key = "3991644640134554:draft:session:197a1b2c3d4:code";
        assert_eq!(extract_session_ts(key), Some(0x197a1b2c3d4));
        assert_eq!(extract_session_ts("no-match"), None);
        assert_eq!(extract_session_ts("u:draft:session:zzz:code"), None);
    }

    #[test]
    fn active_client_returns_none_without_data_roots() {
        // 测试环境无 Trae 数据根（HOME 指向临时目录时），不应 panic。
        // 若本机装有 Trae 则返回 Some，两种结果都合法。
        let _ = active_client();
    }

    #[test]
    fn local_overview_never_panics_and_lists_accounts() {
        let v = local_overview();
        assert!(v.get("accounts").and_then(Value::as_array).is_some());
        assert!(v.get("fetchedAt").and_then(Value::as_i64).is_some());
        assert_eq!(
            v.get("activeClient").and_then(Value::as_str),
            ClientKind::parse(v.get("activeClient").and_then(Value::as_str))
                .map(|k| k.as_str())
        );
    }
}
