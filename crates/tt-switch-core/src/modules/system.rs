//! 系统集成：权限自检、系统设置引导、Finder/资源管理器定位。
//!
//! 全部是本机操作，不接触账号凭据内容。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::modules::config::{backup_dir, store_dir};
use crate::modules::variant::ClientKind;

/// 数据根可写性探针：在客户端数据目录写/删一个探针文件。
///
/// 不直接检查 TCC 授权状态（无公开 API），而是用「能否写文件」这个最终事实判断；
/// 失败时返回带中文引导 hint 的 JSON，供前端展示权限横幅。
pub fn check_data_dir_permission(kind: ClientKind) -> Value {
    let data_root = kind.data_root();
    if !data_root.is_dir() {
        // 目录不存在本身不是权限问题：尝试创建（create_dir_all 同样受 TCC 约束）。
        if let Err(e) = std::fs::create_dir_all(&data_root) {
            return permission_denied_json(kind, &data_root, &e.to_string());
        }
    }
    let probe = data_root.join(".tt-switch-permission.probe");
    match std::fs::write(&probe, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            json!({
                "ok": true,
                "client": kind.as_str(),
                "message": "数据目录可写，权限正常",
            })
        }
        Err(e) => permission_denied_json(kind, &data_root, &e.to_string()),
    }
}

fn permission_denied_json(kind: ClientKind, dir: &Path, error: &str) -> Value {
    json!({
        "ok": false,
        "client": kind.as_str(),
        "error": error,
        "dir": dir.to_string_lossy(),
        "hint": "请打开 系统设置 → 隐私与安全性 → 完全磁盘访问（或 App 管理），把 TT Switch 加进去并开启，授权后需重启 TT Switch 生效",
    })
}

/// 打开 macOS 隐私设置面板（非 macOS 静默无操作）。
#[allow(unused_variables)]
pub fn open_privacy_settings(target: &str) -> Result<(), String> {
    let url = match target {
        "app_management" => {
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AppManagement"
        }
        _ => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles",
    };
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 在 Finder / 资源管理器中定位文件或目录。
pub fn reveal_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| format!("打开 Finder 失败: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn()
            .map_err(|e| format!("打开资源管理器失败: {e}"))?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let dir = if path.is_dir() { path.to_path_buf() } else {
            path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
        };
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("打开文件管理器失败: {e}"))?;
    }
    Ok(())
}

/// 在 Finder 中定位客户端 app 本体（便于拖进系统授权列表）。
pub fn reveal_client_app(kind: ClientKind) -> Result<(), String> {
    let app = crate::modules::process::installed_app_path(kind)
        .ok_or_else(|| format!("未找到 {} 应用", kind.display_name()))?;
    reveal_path(&app)
}

/// 常用路径下发前端（设置页「在 Finder 中显示」用）。
pub fn storage_paths() -> Value {
    json!({
        "storeDir": store_dir().to_string_lossy(),
        "backupsDir": backup_dir().to_string_lossy(),
        "clientDataDirs": ClientKind::ALL
            .iter()
            .map(|k| json!({
                "client": k.as_str(),
                "clientName": k.display_name(),
                "dir": k.data_root().to_string_lossy(),
                "agentToken": k.agent_token_path().to_string_lossy(),
                "exists": k.data_root().is_dir(),
            }))
            .collect::<Vec<Value>>(),
    })
}

/// 备份目录内容清单（新→旧），供设置页展示/手动清理。
pub fn list_backups() -> Vec<Value> {
    let root: PathBuf = backup_dir();
    let mut items: Vec<(std::time::SystemTime, String, u64)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            let size = dir_size(&path);
            items.push((modified, name, size));
        }
    }
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items
        .into_iter()
        .map(|(mtime, name, size)| {
            json!({
                "name": name,
                "atMs": mtime
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
                "sizeBytes": size,
            })
        })
        .collect()
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}
