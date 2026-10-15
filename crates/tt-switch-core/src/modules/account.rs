//! 账号库：读写 `~/.tt-switch/accounts.json`（JSON 数组，原子写）。
//!
//! 账号记录（快照制，区别于 wb-switch 的字段拼装制）：
//! ```json
//! {
//!   "id": "uuid",
//!   "uid": "3991644640134554",
//!   "nickname": "用户标注",
//!   "client": "ide_cn",
//!   "createdAt": 1730000000000,
//!   "capturedAt": 1730000000000,
//!   "expiresAt": 1730000000000
//! }
//! ```
//! 登录态本体存于 `~/.tt-switch/accounts/<id>/` 快照目录，账号库只留元数据。

use serde_json::{json, Value};
use std::path::Path;

use crate::modules::config::{accounts_file, atomic_write, now_ms};
use crate::modules::variant::ClientKind;

fn load_accounts_from_path(path: &Path) -> Vec<Value> {
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(Value::Array(accounts)) = serde_json::from_str::<Value>(&text) {
            return accounts;
        }
    }
    vec![]
}

fn save_accounts_to_path(path: &Path, accounts: &[Value]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(accounts).unwrap_or_default();
    atomic_write(path, &content)
}

/// 读取账号库；文件缺失或损坏返回空列表。
pub fn load_accounts() -> Vec<Value> {
    load_accounts_from_path(&accounts_file())
}

/// 写回账号库（原子写），保持原 JSON 数组结构。
pub fn save_accounts(accounts: &[Value]) -> std::io::Result<()> {
    save_accounts_to_path(&accounts_file(), accounts)
}

/// 按 id 或 uid+client 查找账号。
pub fn find_account(account_id: &str) -> Option<Value> {
    load_accounts()
        .into_iter()
        .find(|account| account.get("id").and_then(Value::as_str) == Some(account_id))
}

/// 取非空字符串字段；空/缺失返回 None。
pub fn get_str(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 账号档位；缺失/未知返回 None。
pub fn client_of(acc: &Value) -> Option<ClientKind> {
    ClientKind::parse(acc.get("client").and_then(Value::as_str))
}

/// 账号展示名（nickname → uid 尾号 → unknown）。
pub fn account_display_name(acc: &Value) -> String {
    if let Some(nick) = get_str(acc, "nickname") {
        return nick;
    }
    if let Some(uid) = get_str(acc, "uid") {
        let tail: String = uid.chars().rev().take(4).collect::<String>().chars().rev().collect();
        return format!("账号 {tail}");
    }
    "unknown".to_string()
}

/// 账号的展示元数据（不含任何凭据）。
pub fn account_meta(acc: &Value) -> Value {
    let expires_at = acc.get("expiresAt").and_then(Value::as_i64);
    json!({
        "id": acc.get("id").cloned().unwrap_or(Value::Null),
        "uid": acc.get("uid").cloned().unwrap_or(Value::Null),
        "nickname": acc.get("nickname").cloned().unwrap_or(Value::Null),
        "displayName": account_display_name(acc),
        "client": acc.get("client").cloned().unwrap_or(Value::Null),
        "clientName": client_of(acc).map(|k| k.display_name()),
        "createdAt": acc.get("createdAt").cloned().unwrap_or(Value::Null),
        "capturedAt": acc.get("capturedAt").cloned().unwrap_or(Value::Null),
        "expiresAt": acc.get("expiresAt").cloned().unwrap_or(Value::Null),
        "needsRelogin": expires_at.map(|exp| exp <= now_ms()).unwrap_or(false),
    })
}

/// 采集结果落库：同 client+uid 已存在则更新快照时间/有效期（保留原 id 与标注），
/// 否则新建记录。返回最终记录。
pub fn upsert_captured_account(
    client: ClientKind,
    uid: &str,
    expires_at: Option<i64>,
    account_id: &str,
) -> std::io::Result<Value> {
    let mut accounts = load_accounts();
    let now = now_ms();
    let existing_pos = accounts.iter().position(|a| {
        a.get("client").and_then(Value::as_str) == Some(client.as_str())
            && a.get("uid").and_then(Value::as_str) == Some(uid)
    });
    if let Some(i) = existing_pos {
        accounts[i]["capturedAt"] = json!(now);
        accounts[i]["expiresAt"] = expires_at.map(|e| json!(e)).unwrap_or(Value::Null);
        // id 以账号库为准（快照目录沿用旧目录）
        let record = accounts[i].clone();
        save_accounts(&accounts)?;
        return Ok(record);
    }
    let record = json!({
        "id": account_id,
        "uid": uid,
        "nickname": "",
        "client": client.as_str(),
        "createdAt": now,
        "capturedAt": now,
        "expiresAt": expires_at.map(|e| json!(e)).unwrap_or(Value::Null),
    });
    accounts.push(record.clone());
    save_accounts(&accounts)?;
    Ok(record)
}

/// 更新标注名。
pub fn rename_account(account_id: &str, nickname: &str) -> Result<(), String> {
    let mut accounts = load_accounts();
    let Some(existing) = accounts
        .iter_mut()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(account_id))
    else {
        return Err("账号不存在".to_string());
    };
    existing["nickname"] = json!(nickname.trim());
    save_accounts(&accounts).map_err(|e| e.to_string())
}

/// 删除账号（账号库记录；快照目录由调用方清理）。
pub fn delete_account(account_id: &str) -> Result<(), String> {
    let mut accounts = load_accounts();
    let before = accounts.len();
    accounts.retain(|a| a.get("id").and_then(Value::as_str) != Some(account_id));
    if accounts.len() == before {
        return Err("账号不存在".to_string());
    }
    save_accounts(&accounts).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_prefers_nickname() {
        let acc = json!({"nickname": "主号", "uid": "3991644640134554"});
        assert_eq!(account_display_name(&acc), "主号");
        let acc = json!({"uid": "3991644640134554"});
        assert_eq!(account_display_name(&acc), "账号 4554");
        let acc = json!({});
        assert_eq!(account_display_name(&acc), "unknown");
    }

    #[test]
    fn meta_never_leaks_token_fields() {
        let acc = json!({
            "id": "x", "uid": "1", "client": "ide_cn",
            "accessToken": "SECRET", "token": "SECRET",
        });
        let meta = account_meta(&acc);
        assert!(meta.get("accessToken").is_none());
        assert!(meta.get("token").is_none());
    }
}
