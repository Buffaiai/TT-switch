//! 简易错误日志：`~/.tt-switch/error-logs.json`（JSON 数组，保留最近 200 条）。

use serde_json::{json, Value};

use crate::modules::config::{atomic_write, error_log_file, now_ms};

const MAX_RECORDS: usize = 200;

/// 追加一条错误日志；失败静默（日志不阻断主流程）。
pub fn log_error(kind: &str, message: &str) {
    let path = error_log_file();
    let mut logs: Vec<Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    logs.push(json!({
        "at": now_ms(),
        "kind": kind,
        "message": message,
    }));
    if logs.len() > MAX_RECORDS {
        logs = logs.split_off(logs.len() - MAX_RECORDS);
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = atomic_write(&path, &serde_json::to_string_pretty(&logs).unwrap_or_default());
}

/// 读取全部错误日志（新→旧）。
pub fn load_error_logs() -> Vec<Value> {
    let mut logs: Vec<Value> = std::fs::read_to_string(error_log_file())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    logs.reverse();
    logs
}

pub fn error_log_path() -> String {
    error_log_file().to_string_lossy().to_string()
}
