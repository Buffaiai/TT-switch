//! 每日签到（checkin_credits）：状态持久化 + 云端协议 + 全账号批量执行。
//!
//! 协议（与 Trae CN IDE 一致）：
//! - `POST {HOST}/trae/api/v2/ug/checkin_credits/status` → `{code:0, enable, checked_in, credits?}`
//! - `POST {HOST}/trae/api/v2/ug/checkin_credits/claim` → `{code:0, ...}`（code≠0 为业务失败）
//! - 头 `Authorization: Cloud-IDE-JWT <token>`；body `{"req_source": 1|2}`（IDE=1，SOLO=2）。
//!
//! 状态文件 `~/.tt-switch/checkin.json`：配置（enabled/time）+ 每账号最近一次结果。
//! Tauri 宿主的后台定时器与 webui 手动触发共享该文件作为协调点（同日幂等）。
//!
//! 合规：token 仅在内存中用于请求头，不落日志、不落盘。

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::modules::account;
use crate::modules::config::{self, now_ms};
use crate::modules::snapshot::{self, AGENT_TOKEN_SNAPSHOT_NAME};
use crate::modules::variant::ClientKind;

const HOST: &str = "https://api.trae.cn";
const STATUS_PATH: &str = "/trae/api/v2/ug/checkin_credits/status";
const CLAIM_PATH: &str = "/trae/api/v2/ug/checkin_credits/claim";
const HTTP_TIMEOUT_SECS: u64 = 12;
/// 失败后的当日重试最小间隔（避免每轮 tick 都打失败账号）。
const ERROR_RETRY_MIN_MS: i64 = 10 * 60 * 1000;

static RUNNING: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// 状态文件
// ---------------------------------------------------------------------------

fn state_file() -> PathBuf {
    config::store_dir().join("checkin.json")
}

fn default_state() -> Value {
    json!({
        "enabled": true,
        "time": "09:00",
        "deviceId": uuid::Uuid::new_v4().to_string(),
        "accounts": {},
    })
}

/// 解析状态文本；损坏或缺字段时降级为默认值（缺省字段逐个补齐）。
fn parse_state(text: &str) -> Value {
    let mut v: Value = serde_json::from_str(text).unwrap_or_else(|_| default_state());
    if !v.is_object() {
        return default_state();
    }
    if v.get("enabled").and_then(Value::as_bool).is_none() {
        v["enabled"] = json!(true);
    }
    if v.get("time").and_then(Value::as_str).map(valid_time_str) != Some(true) {
        v["time"] = json!("09:00");
    }
    if v.get("deviceId").and_then(Value::as_str).is_none() {
        v["deviceId"] = json!(uuid::Uuid::new_v4().to_string());
    }
    if v.get("accounts").and_then(Value::as_object).is_none() {
        v["accounts"] = json!({});
    }
    v
}

pub fn load_state() -> Value {
    match std::fs::read_to_string(state_file()) {
        Ok(text) => parse_state(&text),
        Err(_) => default_state(),
    }
}

pub fn save_state(state: &Value) -> std::io::Result<()> {
    let f = state_file();
    if let Some(parent) = f.parent() {
        std::fs::create_dir_all(parent)?;
    }
    config::atomic_write(&f, &serde_json::to_string_pretty(state).unwrap_or_default())
}

/// 校验 HH:MM（24h）。
fn valid_time_str(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    let Ok(h) = parts[0].parse::<u32>() else {
        return false;
    };
    let Ok(m) = parts[1].parse::<u32>() else {
        return false;
    };
    h < 24 && m < 60 && parts[0].len() == 2 && parts[1].len() == 2
}

pub fn today_str() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

// ---------------------------------------------------------------------------
// 云端协议
// ---------------------------------------------------------------------------

/// 签到仅 CN 档位可用：IDE=1，SOLO=2，国际版跳过。
pub fn req_source_of(client: Option<&str>) -> Option<u8> {
    match ClientKind::parse(client) {
        Some(ClientKind::IdeCn) => Some(1),
        Some(ClientKind::SoloCn) => Some(2),
        _ => None,
    }
}

fn read_account_token(account_id: &str) -> Option<String> {
    let path = snapshot::snapshot_dir(account_id).join(AGENT_TOKEN_SNAPSHOT_NAME);
    std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent(concat!("tt-switch/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("http client init failed: {e}"))
}

/// 调签到接口；返回 (http_status, body)。token 仅放入请求头，不打印。
async fn post_checkin(
    token: &str,
    path: &str,
    req_source: u8,
    device_id: &str,
) -> Result<(u16, Value), String> {
    let client = http_client()?;
    let url = format!("{HOST}{path}");
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("Authorization", format!("Cloud-IDE-JWT {token}"))
        .header("x-device-id", device_id)
        .json(&json!({ "req_source": req_source }))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({}));
    Ok((status, body))
}

struct StatusInfo {
    enable: bool,
    checked_in: bool,
    credits: Option<i64>,
}

/// 解析 status 响应；code≠0 或字段缺失视为协议错误。
fn interpret_status(v: &Value) -> Result<StatusInfo, String> {
    let code = v.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let msg = v.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("business code {code}: {msg}"));
    }
    Ok(StatusInfo {
        enable: v.get("enable").and_then(Value::as_bool).unwrap_or(false),
        checked_in: v.get("checked_in").and_then(Value::as_bool).unwrap_or(false),
        credits: v.get("credits").and_then(Value::as_i64),
    })
}

/// 解析 claim 响应；code≠0 视为业务失败。返回 credits（若有）。
fn interpret_claim(v: &Value) -> Result<Option<i64>, String> {
    let code = v.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let msg = v.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("business code {code}: {msg}"));
    }
    Ok(v.get("credits").and_then(Value::as_i64))
}

// ---------------------------------------------------------------------------
// 幂等与重试判定（纯函数，供单测）
// ---------------------------------------------------------------------------

/// 今日是否已终结（ok / already），或处于失败重试窗口内 → 跳过。
fn should_skip(rec: Option<&Value>, today: &str, now: i64) -> bool {
    let Some(rec) = rec else {
        return false;
    };
    if rec.get("last_claim_date").and_then(Value::as_str) != Some(today) {
        return false;
    }
    match rec.get("last_result").and_then(Value::as_str) {
        Some("ok") | Some("already") => true,
        Some("error") => {
            let last_run = rec.get("last_run_ms").and_then(Value::as_i64).unwrap_or(0);
            now - last_run < ERROR_RETRY_MIN_MS
        }
        _ => false,
    }
}

fn account_rec<'a>(state: &'a Value, id: &str) -> Option<&'a Value> {
    state.get("accounts")?.get(id)
}

fn set_account_rec(state: &mut Value, id: &str, result: &str, error: &str, credits: Option<i64>) {
    let today = today_str();
    let rec = json!({
        "last_claim_date": today,
        "last_result": result,
        "last_error": error,
        "last_credits": credits,
        "last_run_ms": now_ms(),
    });
    state["accounts"][id] = rec;
}

// ---------------------------------------------------------------------------
// 批量执行
// ---------------------------------------------------------------------------

/// 全账号签到。`force` 仅由手动触发使用（绕过时间检查）；同日幂等始终生效。
///
/// 每账号处理后立即落盘，所有提前退出路径都会收尾状态文件。
pub async fn run_all(force: bool) -> Value {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return json!({ "ok": false, "error": "already running" });
    }
    let result = run_all_inner(force).await;
    RUNNING.store(false, Ordering::SeqCst);
    result
}

async fn run_all_inner(_force: bool) -> Value {
    let mut state = load_state();
    let today = today_str();
    let now = now_ms();
    let device_id = state
        .get("deviceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut results: Vec<Value> = Vec::new();

    for acc in account::load_accounts() {
        let Some(id) = account::get_str(&acc, "id") else {
            continue;
        };
        let name = account::account_display_name(&acc);
        let Some(req_source) = req_source_of(account::get_str(&acc, "client").as_deref()) else {
            continue; // 非 CN 档位无签到活动
        };
        if should_skip(account_rec(&state, &id), &today, now) {
            results.push(json!({ "id": id, "name": name, "result": "skipped" }));
            continue;
        }

        let outcome: (String, String, Option<i64>) = match read_account_token(&id) {
            None => ("error".into(), "no_token".into(), None),
            Some(token) => match post_checkin(&token, STATUS_PATH, req_source, &device_id).await {
                Err(e) => ("error".into(), e, None),
                Ok((401, _)) => ("error".into(), "token_expired".into(), None),
                Ok((http, _)) if http != 200 => ("error".into(), format!("http {http}"), None),
                Ok((_, body)) => match interpret_status(&body) {
                    Err(e) => ("error".into(), e, None),
                    Ok(st) if !st.enable => ("error".into(), "activity_disabled".into(), st.credits),
                    Ok(st) if st.checked_in => ("already".into(), String::new(), st.credits),
                    Ok(st) => {
                        match post_checkin(&token, CLAIM_PATH, req_source, &device_id).await {
                            Err(e) => ("error".into(), e, None),
                            Ok((401, _)) => ("error".into(), "token_expired".into(), None),
                            Ok((http, _)) if http != 200 => {
                                ("error".into(), format!("http {http}"), None)
                            }
                            Ok((_, body)) => match interpret_claim(&body) {
                                Ok(credits) => ("ok".into(), String::new(), credits.or(st.credits)),
                                Err(e) => ("error".into(), e, None),
                            },
                        }
                    }
                },
            },
        };

        set_account_rec(&mut state, &id, &outcome.0, &outcome.1, outcome.2);
        // 每账号处理后落盘：进程中断也不丢失已完成账号的幂等标记。
        let _ = save_state(&state);
        results.push(json!({
            "id": id,
            "name": name,
            "result": outcome.0,
            "error": outcome.1,
            "credits": outcome.2,
        }));
    }

    let _ = save_state(&state);
    json!({ "ok": true, "today": today, "results": results })
}

// ---------------------------------------------------------------------------
// 对外查询 / 配置
// ---------------------------------------------------------------------------

/// 前端徽章与签到卡片数据：配置 + 今日 + 每账号最近结果。
pub fn status_summary() -> Value {
    let state = load_state();
    json!({
        "ok": true,
        "today": today_str(),
        "enabled": state.get("enabled").cloned().unwrap_or(json!(true)),
        "time": state.get("time").cloned().unwrap_or(json!("09:00")),
        "accounts": state.get("accounts").cloned().unwrap_or(json!({})),
    })
}

/// 更新配置（开关 / 时间），返回最新 summary。非法时间报错。
pub fn set_config(enabled: Option<bool>, time: Option<&str>) -> Result<Value, String> {
    let mut state = load_state();
    if let Some(t) = time {
        if !valid_time_str(t) {
            return Err(format!("无效时间格式（需 HH:MM）: {t}"));
        }
        state["time"] = json!(t);
    }
    if let Some(e) = enabled {
        state["enabled"] = json!(e);
    }
    save_state(&state).map_err(|e| format!("save failed: {e}"))?;
    Ok(status_summary())
}

/// 后台定时器 tick：到点且今日未全部终结则执行。
///
/// 返回是否触发了执行（调用方无需关心细节，仅供日志/测试）。
pub async fn scheduler_tick() -> bool {
    let state = load_state();
    if state.get("enabled").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    let cfg_time = state
        .get("time")
        .and_then(Value::as_str)
        .unwrap_or("09:00")
        .to_string();
    let now_hm = chrono::Local::now().format("%H:%M").to_string();
    if now_hm < cfg_time {
        return false;
    }
    let today = today_str();
    let now = now_ms();
    // 存在任一「今日未终结」的 CN 账号即触发。
    let pending = account::load_accounts().iter().any(|acc| {
        let Some(id) = account::get_str(acc, "id") else {
            return false;
        };
        req_source_of(account::get_str(acc, "client").as_deref()).is_some()
            && !should_skip(account_rec(&state, &id), &today, now)
    });
    if !pending {
        return false;
    }
    let _ = run_all(false).await;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_time_accepts_hh_mm() {
        assert!(valid_time_str("09:00"));
        assert!(valid_time_str("23:59"));
        assert!(!valid_time_str("9:00"));
        assert!(!valid_time_str("24:00"));
        assert!(!valid_time_str("12:60"));
        assert!(!valid_time_str("0900"));
        assert!(!valid_time_str(""));
    }

    #[test]
    fn parse_state_degrades_on_corrupt_and_fills_defaults() {
        let v = parse_state("not-json{{{");
        assert_eq!(v.get("enabled").and_then(Value::as_bool), Some(true));
        assert_eq!(v.get("time").and_then(Value::as_str), Some("09:00"));
        assert!(v.get("deviceId").and_then(Value::as_str).is_some());
        assert!(v.get("accounts").and_then(Value::as_object).is_some());

        let v = parse_state(r#"{"enabled":false,"time":"bad","accounts":{"a":{"last_result":"ok"}}}"#);
        assert_eq!(v.get("enabled").and_then(Value::as_bool), Some(false));
        assert_eq!(v.get("time").and_then(Value::as_str), Some("09:00"));
        assert!(v["accounts"].get("a").is_some());
    }

    #[test]
    fn req_source_maps_cn_clients_only() {
        assert_eq!(req_source_of(Some("ide_cn")), Some(1));
        assert_eq!(req_source_of(Some("solo_cn")), Some(2));
        assert_eq!(req_source_of(Some("ide_intl")), None);
        assert_eq!(req_source_of(None), None);
        assert_eq!(req_source_of(Some("unknown")), None);
    }

    #[test]
    fn interpret_status_parses_fields() {
        let v = json!({"code":0,"enable":true,"checked_in":false,"credits":50});
        let st = interpret_status(&v).unwrap();
        assert!(st.enable && !st.checked_in && st.credits == Some(50));

        // 缺 credits 合法
        let v = json!({"code":0,"enable":true,"checked_in":true});
        let st = interpret_status(&v).unwrap();
        assert!(st.checked_in && st.credits.is_none());

        // 业务错误
        assert!(interpret_status(&json!({"code":1001,"message":"auth"})).is_err());
        // 缺 code 视为协议错误
        assert!(interpret_status(&json!({"enable":true})).is_err());
    }

    #[test]
    fn interpret_claim_requires_zero_code() {
        assert_eq!(interpret_claim(&json!({"code":0,"credits":10})).unwrap(), Some(10));
        assert_eq!(interpret_claim(&json!({"code":0})).unwrap(), None);
        assert!(interpret_claim(&json!({"code":500,"message":"x"})).is_err());
    }

    #[test]
    fn should_skip_enforces_idempotency_and_retry_window() {
        let today = "2026-10-14";
        let now = 1_000_000_000_000i64;
        // 无记录不跳过
        assert!(!should_skip(None, today, now));
        // 今日 ok / already → 跳过
        let ok = json!({"last_claim_date":today,"last_result":"ok","last_run_ms":now});
        assert!(should_skip(Some(&ok), today, now));
        let already = json!({"last_claim_date":today,"last_result":"already","last_run_ms":now});
        assert!(should_skip(Some(&already), today, now));
        // 昨日 ok → 不跳过（跨天重置）
        let yesterday = json!({"last_claim_date":"2026-10-13","last_result":"ok","last_run_ms":now});
        assert!(!should_skip(Some(&yesterday), today, now));
        // 今日 error：窗口内跳过，窗口外重试
        let err_recent = json!({"last_claim_date":today,"last_result":"error","last_run_ms":now - 5*60*1000});
        assert!(should_skip(Some(&err_recent), today, now));
        let err_old = json!({"last_claim_date":today,"last_result":"error","last_run_ms":now - 11*60*1000});
        assert!(!should_skip(Some(&err_old), today, now));
    }
}
