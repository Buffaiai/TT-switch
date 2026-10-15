//! 路径、时间、原子写等基础设施。存储根 `~/.tt-switch/`。

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::modules::variant::ClientKind;

// ---------------------------------------------------------------------------
// 路径
// ---------------------------------------------------------------------------

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn store_dir() -> PathBuf {
    home_dir().join(".tt-switch")
}

pub fn accounts_file() -> PathBuf {
    store_dir().join("accounts.json")
}

/// 账号快照根：`~/.tt-switch/accounts/<id>/`。
pub fn snapshots_dir() -> PathBuf {
    store_dir().join("accounts")
}

/// 切换前对「当前登录态」的安全备份根（非账号快照，仅用于事故恢复）。
pub fn backup_dir() -> PathBuf {
    store_dir().join("backups")
}

pub fn error_log_file() -> PathBuf {
    store_dir().join("error-logs.json")
}

pub fn app_cache_file() -> PathBuf {
    store_dir().join("app_paths.json")
}

// ---------------------------------------------------------------------------
// app 路径缓存（按档位分键）
// ---------------------------------------------------------------------------

/// 读取上次成功解析到的 app 路径；损坏或空文件视为无缓存。
pub fn load_app_cache(kind: ClientKind) -> Option<PathBuf> {
    let f = app_cache_file();
    let text = std::fs::read_to_string(f).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let raw = v.get(kind.app_cache_key())?.as_str()?.trim();
    if raw.is_empty() {
        None
    } else {
        Some(PathBuf::from(raw))
    }
}

pub fn save_app_cache(kind: ClientKind, path: &Path) -> std::io::Result<()> {
    let f = app_cache_file();
    let mut v: Value = std::fs::read_to_string(&f)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    if !v.is_object() {
        v = Value::Object(serde_json::Map::new());
    }
    v[kind.app_cache_key()] = Value::String(path.to_string_lossy().to_string());
    if let Some(parent) = f.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&f, &serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub fn clear_app_cache(kind: ClientKind) {
    let f = app_cache_file();
    let Ok(text) = std::fs::read_to_string(&f) else {
        return;
    };
    let Ok(mut v) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    if let Some(map) = v.as_object_mut() {
        map.remove(kind.app_cache_key());
        let _ = atomic_write(&f, &serde_json::to_string_pretty(&v).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------------
// 时间
// ---------------------------------------------------------------------------

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn utc_iso() -> String {
    // %Y-%m-%dT%H-%M-%S + "Z"（文件名安全）
    format!("{}Z", chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S"))
}

// ---------------------------------------------------------------------------
// 文件
// ---------------------------------------------------------------------------

/// 原子写文件（临时文件 + rename）。
pub fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{file_name}.tmp-{}", uuid::Uuid::new_v4().simple()));
    if let Err(e) = std::fs::write(&tmp, content) {
        eprintln!("[atomic] write tmp FAILED: {e}");
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        eprintln!("[atomic] rename FAILED: {e}");
        // rename 失败时清理临时文件，避免在目标目录残留 `<name>.tmp-*`。
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tt-switch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.json");
        atomic_write(&f, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{}");
        // 不留临时文件
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
