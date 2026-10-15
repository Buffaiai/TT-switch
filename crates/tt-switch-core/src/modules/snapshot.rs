//! 登录态快照：把某客户端当前登录态整文件**不透明**拷贝到
//! `~/.tt-switch/accounts/<id>/`，切换时再整体写回原位。
//!
//! 合规红线：本模块只拷贝/删除文件本身，绝不读取、打印 token / Cookies 内容。
//! 唯一例外是解析 agent token 的 JWT **payload**（user_id / exp）用于账号元数据——
//! 解析结果不落盘 token 本体，日志也只输出 uid 与过期时间。

use base64::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::modules::account;
use crate::modules::config::{backup_dir, snapshots_dir, utc_iso};
use crate::modules::variant::{ClientKind, IDE_SNAPSHOT_DIRS, IDE_SNAPSHOT_FILES};

/// 快照内 agent token 的存放名（位于快照根，不入 IDE 子目录）。
pub const AGENT_TOKEN_SNAPSHOT_NAME: &str = "agent-token.bin";
const MANIFEST_NAME: &str = "manifest.json";

/// 从 JWT 解析出的账号身份（只保留非敏感字段）。
#[derive(Debug, Clone)]
pub struct JwtIdentity {
    pub user_id: String,
    /// 过期时间（毫秒）；payload 无 exp 时为 None。
    pub expires_at: Option<i64>,
}

/// 解析 agent token 的 JWT payload（base64url），提取 user_id / exp。
///
/// 只读 payload 段，不验证签名（本地元数据用途）；token 内容不出本函数。
pub fn parse_jwt_identity(token_path: &Path) -> Option<JwtIdentity> {
    let text = std::fs::read_to_string(token_path).ok()?;
    let token = text.trim();
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    // user_id 兼容两种形态：顶层 `user_id`，或 Trae 现行的 `data.user_id` 嵌套。
    let pick = |obj: &Value| {
        obj.get("user_id")
            .and_then(|u| u.as_str().map(str::to_string).or_else(|| u.as_i64().map(|n| n.to_string())))
            .filter(|s| !s.is_empty())
    };
    let user_id = pick(&v).or_else(|| v.get("data").and_then(pick))?;
    let expires_at = v
        .get("exp")
        .and_then(Value::as_i64)
        .map(|secs| secs * 1000);
    Some(JwtIdentity {
        user_id,
        expires_at,
    })
}

/// 账号快照目录。
pub fn snapshot_dir(account_id: &str) -> PathBuf {
    snapshots_dir().join(account_id)
}

/// 递归复制目录内容（先清目标，避免残留旧文件串号）。
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() {
        std::fs::remove_dir_all(dst)?;
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            // 目标已建空目录，子目录逐个复制（不先清，因为 to 尚不存在）。
            copy_dir_into(&from, &to)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to)?;
        }
        // 符号链接/特殊文件跳过：登录态目录里不应出现，跟随链接有越界风险。
    }
    Ok(())
}

fn copy_dir_into(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_into(&from, &to)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 路径是否是活动 SQLite 登录态库（state.vscdb / Chromium Cookies）。
fn is_sqlite_login_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    name.ends_with(".vscdb") || name == "Cookies"
}

/// 用 SQLite 在线备份 API 把活动库复制为事务一致的单文件快照。
///
/// 源库以只读方式打开（不干扰 IDE 的写连接，也不需要关 IDE）；
/// 目标是全新库，备份结果自带全部已提交页，不依赖 -wal/-shm。
/// 失败时返回 Err，由调用方回退为普通文件拷贝（如文件根本不是合法 SQLite）。
fn sqlite_consistent_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    use rusqlite::backup::Backup;
    use rusqlite::{Connection, OpenFlags};

    let to_io = |e: rusqlite::Error| std::io::Error::new(std::io::ErrorKind::Other, e);
    let source = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(to_io)?;
    let mut dest = Connection::open(dst).map_err(to_io)?;
    {
        let backup = Backup::new(&source, &mut dest).map_err(to_io)?;
        // 每轮最多 64 页，不挂进度回调（本地文件，尽快跑完）。
        backup
            .run_to_completion(64, std::time::Duration::from_millis(0), None)
            .map_err(to_io)?;
    }
    drop(source);
    drop(dest);
    Ok(())
}

/// 采集单个登录态文件：SQLite 库优先在线备份，失败回退普通拷贝。
/// 返回清单 kind（"sqlite" / "file"）。
fn copy_login_file(src: &Path, dst: &Path) -> std::io::Result<&'static str> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 在线备份的目标必须不存在/为空库：先清掉残留。
    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    if is_sqlite_login_file(src) {
        match sqlite_consistent_copy(src, dst) {
            Ok(()) => return Ok("sqlite"),
            Err(e) => {
                eprintln!("[snapshot] sqlite online backup failed for {}: {e}; fallback to copy", src.display());
            }
        }
    }
    std::fs::copy(src, dst)?;
    Ok("file")
}

/// 采集当前登录态为账号快照。返回 (account_record, snapshot_entry_count)。
///
/// 要求：数据根存在（说明该客户端至少登录过一次）、agent token 可解析出 uid。
/// 已存在同 client+uid 账号时刷新其快照（目录沿用原 id）。
pub fn capture_account(kind: ClientKind) -> Result<Value, String> {
    let data_root = kind.data_root();
    if !data_root.is_dir() {
        return Err(format!(
            "未检测到 {} 的数据目录（{}），请先登录一次该客户端",
            kind.display_name(),
            data_root.display()
        ));
    }
    let token_path = kind.agent_token_path();
    let identity = parse_jwt_identity(&token_path).ok_or_else(|| {
        format!(
            "未能从 agent token 解析账号身份（{}），请先登录一次 {}（含 Trae/Agent）",
            token_path.display(),
            kind.display_name()
        )
    })?;

    // 落库（同 client+uid 复用原 id），再按最终 id 采集快照。
    let provisional_id = uuid::Uuid::new_v4().to_string();
    let record = account::upsert_captured_account(
        kind,
        &identity.user_id,
        identity.expires_at,
        &provisional_id,
    )
    .map_err(|e| format!("账号库写入失败: {e}"))?;
    let id = account::get_str(&record, "id").unwrap_or(provisional_id);

    let dest = snapshot_dir(&id);
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| format!("清理旧快照失败: {e}"))?;
    }
    std::fs::create_dir_all(&dest).map_err(|e| format!("创建快照目录失败: {e}"))?;

    let mut entries: Vec<Value> = Vec::new();
    for rel in IDE_SNAPSHOT_FILES {
        let src = data_root.join(rel);
        if src.is_file() {
            let to = dest.join(rel);
            let kind = copy_login_file(&src, &to).map_err(|e| format!("复制 {rel} 失败: {e}"))?;
            entries.push(json!({"path": rel, "kind": kind}));
        }
    }
    for rel in IDE_SNAPSHOT_DIRS {
        let src = data_root.join(rel);
        if src.is_dir() {
            copy_dir_all(&src, &dest.join(rel)).map_err(|e| format!("复制 {rel} 失败: {e}"))?;
            entries.push(json!({"path": rel, "kind": "dir"}));
        }
    }
    if entries.is_empty() {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(format!(
            "{} 数据目录下未找到登录态文件，请先登录一次该客户端",
            kind.display_name()
        ));
    }

    // agent token 整文件入快照（不读内容）。
    let mut has_token = false;
    if token_path.is_file() {
        std::fs::copy(&token_path, dest.join(AGENT_TOKEN_SNAPSHOT_NAME))
            .map_err(|e| format!("复制 agent token 失败: {e}"))?;
        has_token = true;
    }

    let manifest = json!({
        "client": kind.as_str(),
        "uid": identity.user_id,
        "capturedAt": crate::modules::config::now_ms(),
        "entries": entries,
        "agentToken": has_token,
    });
    crate::modules::config::atomic_write(
        &dest.join(MANIFEST_NAME),
        &serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(|e| format!("写快照清单失败: {e}"))?;

    Ok(record)
}

/// 把账号快照写回该客户端数据根与 agent token 路径。调用方须保证目标 app 已关闭。
pub fn restore_account(acc: &Value) -> Result<usize, String> {
    let kind = account::client_of(acc).ok_or("账号记录缺少有效 client 字段")?;
    let id = account::get_str(acc, "id").ok_or("账号记录缺少 id")?;
    let dir = snapshot_dir(&id);
    if !dir.is_dir() {
        return Err("账号快照目录不存在（可能已被清理），请重新添加该账号".to_string());
    }
    let manifest_text = std::fs::read_to_string(dir.join(MANIFEST_NAME))
        .map_err(|e| format!("读取快照清单失败: {e}"))?;
    let manifest: Value =
        serde_json::from_str(&manifest_text).map_err(|e| format!("快照清单损坏: {e}"))?;

    let data_root = kind.data_root();
    std::fs::create_dir_all(&data_root).map_err(|e| e.to_string())?;

    /// 清掉目标库的残留 WAL：强杀后可能留下未 checkpoint 的 -wal/-shm，
    /// 若直接覆盖主库，SQLite 会拿旧 WAL 重放，造成「恢复了但账号没变」甚至库损坏。
    fn remove_sqlite_sidecars(path: &Path) {
        let Some(name) = path.file_name() else {
            return;
        };
        let n = name.to_string_lossy();
        for suffix in ["-wal", "-shm"] {
            let side = path.with_file_name(format!("{n}{suffix}"));
            if side.is_file() {
                let _ = std::fs::remove_file(&side);
            }
        }
    }

    /// 写后校验：快照与落位文件字节数必须一致（不读内容，合规且足以发现截断/权限问题）。
    fn verify_same_size(src: &Path, dst: &Path, rel: &str) -> Result<(), String> {
        let s = std::fs::metadata(src).map_err(|e| format!("校验 {rel} 失败: {e}"))?;
        let d = std::fs::metadata(dst).map_err(|e| format!("校验 {rel} 失败: {e}"))?;
        if s.len() != d.len() {
            return Err(format!("恢复 {rel} 后校验失败：字节数不一致（{} != {}）", s.len(), d.len()));
        }
        Ok(())
    }

    let mut restored = 0usize;
    for entry in manifest
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let Some(rel) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        // 防路径逃逸：清单里的相对路径不允许跳出数据根。
        if rel.split('/').any(|seg| seg == "..") {
            return Err(format!("快照清单含非法路径: {rel}"));
        }
        let src = dir.join(rel);
        let dst = data_root.join(rel);
        match entry.get("kind").and_then(Value::as_str) {
            Some("dir") => {
                if src.is_dir() {
                    copy_dir_all(&src, &dst).map_err(|e| format!("恢复 {rel} 失败: {e}"))?;
                    restored += 1;
                }
            }
            _ => {
                if src.is_file() {
                    if let Some(parent) = dst.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    // 目标 app 已关闭，普通整文件拷贝即可；先清残留 WAL 侧车文件。
                    remove_sqlite_sidecars(&dst);
                    std::fs::copy(&src, &dst).map_err(|e| format!("恢复 {rel} 失败: {e}"))?;
                    verify_same_size(&src, &dst, rel)?;
                    restored += 1;
                }
            }
        }
    }

    // agent token 写回（0600）。Trae/Agent 端只依赖该文件，无需重启 IDE。
    let token_snapshot = dir.join(AGENT_TOKEN_SNAPSHOT_NAME);
    if token_snapshot.is_file() {
        let token_path = kind.agent_token_path();
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&token_snapshot, &token_path)
            .map_err(|e| format!("恢复 agent token 失败: {e}"))?;
        verify_same_size(&token_snapshot, &token_path, "agent token")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600));
        }
    }

    Ok(restored)
}

/// 切换前保留的安全备份份数（按客户端分别计数）。
const BACKUPS_KEEP_PER_CLIENT: usize = 10;

/// 切换前把「当前登录态」安全备份到 `~/.tt-switch/backups/<client>-<ts>/`。
/// 仅用于事故恢复，不进账号库；失败只告警不阻断切换。返回备份目录。
pub fn backup_current_state(kind: ClientKind) -> Option<PathBuf> {
    let data_root = kind.data_root();
    if !data_root.is_dir() {
        return None;
    }
    let dest = backup_dir().join(format!("{}-{}", kind.as_str(), utc_iso()));
    std::fs::create_dir_all(&dest).ok()?;
    let mut copied = 0usize;
    for rel in IDE_SNAPSHOT_FILES {
        let src = data_root.join(rel);
        if src.is_file() {
            // 与账号采集同一套：SQLite 走在线备份，保证备份在事故恢复时可用。
            if copy_login_file(&src, &dest.join(rel)).is_ok() {
                copied += 1;
            }
        }
    }
    for rel in IDE_SNAPSHOT_DIRS {
        let src = data_root.join(rel);
        if src.is_dir() && copy_dir_all(&src, &dest.join(rel)).is_ok() {
            copied += 1;
        }
    }
    let token_path = kind.agent_token_path();
    if token_path.is_file() {
        let _ = std::fs::copy(&token_path, dest.join(AGENT_TOKEN_SNAPSHOT_NAME));
    }
    if copied == 0 {
        let _ = std::fs::remove_dir_all(&dest);
        return None;
    }
    prune_backups(kind);
    Some(dest)
}

/// 清理某客户端的旧安全备份，按目录名（含 UTC 时间戳）保留最新 N 份。
fn prune_backups(kind: ClientKind) {
    let root = backup_dir();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let prefix = format!("{}-", kind.as_str());
    let mut dirs: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with(&prefix))
        .collect();
    // 名称格式 client-YYYY-..Z，字典序即时间序。
    dirs.sort();
    let total = dirs.len();
    if total <= BACKUPS_KEEP_PER_CLIENT {
        return;
    }
    for name in dirs.into_iter().take(total - BACKUPS_KEEP_PER_CLIENT) {
        let _ = std::fs::remove_dir_all(root.join(&name));
    }
}

/// 删除账号快照目录。
pub fn remove_snapshot(account_id: &str) {
    let dir = snapshot_dir(account_id);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// 当前登录态身份（状态页展示）：agent token 解析结果。
pub fn current_identity(kind: ClientKind) -> Option<JwtIdentity> {
    parse_jwt_identity(&kind.agent_token_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_jwt(payload: &str) -> (PathBuf, String) {
        // header.payload.signature —— payload 传入 base64url 原文
        let dir = std::env::temp_dir().join(format!("tt-switch-jwt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let token = format!("e30.{payload}.sig");
        let path = dir.join("trae-jwt-token");
        std::fs::write(&path, &token).unwrap();
        (path, token)
    }

    #[test]
    fn parse_identity_from_payload() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"user_id":"3991644640134554","exp":1730000000}"#);
        let (path, _) = write_jwt(&payload);
        let id = parse_jwt_identity(&path).unwrap();
        assert_eq!(id.user_id, "3991644640134554");
        assert_eq!(id.expires_at, Some(1730000000 * 1000));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn parse_identity_rejects_garbage() {
        let (path, _) = write_jwt("not-base64!!!");
        assert!(parse_jwt_identity(&path).is_none());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn parse_identity_requires_user_id() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"exp":1730000000}"#);
        let (path, _) = write_jwt(&payload);
        assert!(parse_jwt_identity(&path).is_none());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Trae 现行形态：user_id 嵌套在 data 对象里。
    #[test]
    fn parse_identity_nested_data() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"data":{"user_id":"3991644640134554","tenant_id":"t"},"exp":1730000000,"iat":1,"iss":"x"}"#);
        let (path, _) = write_jwt(&payload);
        let id = parse_jwt_identity(&path).unwrap();
        assert_eq!(id.user_id, "3991644640134554");
        assert_eq!(id.expires_at, Some(1730000000 * 1000));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
