//! 账号切换编排：互斥 → 备份当前登录态 → 关进程 → 恢复快照 → 启动 → 返回报告。
//!
//! 切换过程中通过进度回调向前端推送实时进度，同时写入内存进度缓存，
//! 供 webui 轮询（GET /api/switch/progress）与桌面端轮询共用。

use serde_json::{json, Value};
use std::sync::Mutex;

use crate::modules::account;
use crate::modules::process::{close_client, is_running, launch_client};
use crate::modules::snapshot;
use crate::modules::variant::ClientKind;

/// 切换进度回调（宿主注入）。
pub type ProgressFn = Box<dyn Fn(&str) + Send + Sync>;

/// 全局操作锁：同一时刻只允许一个切换/采集在执行。
static SWITCH_LOCK: Mutex<()> = Mutex::new(());

/// 进度缓存：webui 轮询用。结构 {running, messages: [..]}。
static PROGRESS: Mutex<Value> = Mutex::new(Value::Null);

fn progress_reset() {
    let mut guard = PROGRESS.lock().unwrap();
    *guard = json!({"running": true, "messages": []});
}

fn progress_push(msg: &str) {
    let mut guard = PROGRESS.lock().unwrap();
    if !guard.is_object() {
        *guard = json!({"running": true, "messages": []});
    }
    if let Some(arr) = guard.get_mut("messages").and_then(Value::as_array_mut) {
        arr.push(json!({
            "at": crate::modules::config::now_ms(),
            "message": msg,
        }));
    }
}

fn progress_finish() {
    let mut guard = PROGRESS.lock().unwrap();
    if let Some(obj) = guard.as_object_mut() {
        obj.insert("running".to_string(), json!(false));
    }
}

/// 进度快照（含进行中标记与全部消息）。
pub fn progress_snapshot() -> Value {
    let guard = PROGRESS.lock().unwrap();
    if guard.is_object() {
        guard.clone()
    } else {
        json!({"running": false, "messages": []})
    }
}

/// 采集当前登录态为账号（带互斥与进度）。
pub fn capture_current(kind: ClientKind) -> Result<Value, String> {
    let _guard = match SWITCH_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return Err("另一个操作正在进行中，请稍后再试".to_string()),
    };
    progress_reset();
    progress_push(&format!("正在采集 {} 当前登录态…", kind.display_name()));
    let result = snapshot::capture_account(kind);
    match &result {
        Ok(record) => {
            progress_push(&format!(
                "已保存账号 {}（uid {}）",
                account::account_display_name(record),
                account::get_str(record, "uid").unwrap_or_default()
            ));
        }
        Err(e) => progress_push(&format!("采集失败: {e}")),
    }
    progress_finish();
    result
}

/// 切换账号：恢复目标账号快照并重启客户端。
///
/// `restart=false` 仅恢复快照不启动（用户自行打开）。
/// 幂等：目标账号就是当前登录态时直接成功返回，不动文件、不关进程。
pub fn switch_account(account_id: &str, restart: bool, progress: Option<ProgressFn>) -> Result<Value, String> {
    let _guard = match SWITCH_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return Err("另一个切换正在进行中，请稍后再试".to_string()),
    };

    // 入参校验放在进度重置之前：早退不污染进度状态。
    let acc = account::find_account(account_id).ok_or("账号不存在")?;
    let kind = account::client_of(&acc).ok_or("账号记录缺少有效 client 字段")?;
    let name = account::account_display_name(&acc);
    let target_uid = account::get_str(&acc, "uid");

    // 幂等短路：当前 agent token 身份就是目标账号 → 无需任何操作。
    let current_uid = snapshot::current_identity(kind).map(|i| i.user_id);
    if current_uid.is_some() && current_uid == target_uid {
        progress_reset();
        let say = |msg: &str| {
            progress_push(msg);
            if let Some(p) = &progress {
                p(msg);
            }
        };
        say(&format!("当前登录的就是「{name}」，无需切换"));
        progress_finish();
        return Ok(json!({
            "ok": true,
            "account": account::account_meta(&acc),
            "client": kind.as_str(),
            "restarted": false,
            "alreadyCurrent": true,
        }));
    }

    progress_reset();
    let say = |msg: &str| {
        progress_push(msg);
        if let Some(p) = &progress {
            p(msg);
        }
    };

    // 破坏性流程：无论成败都必须结束进度状态，避免前端卡在「进行中」。
    let outcome = (|| -> Result<usize, String> {
        // 1) 备份当前登录态（失败只告警）
        say("正在备份当前登录态…");
        match snapshot::backup_current_state(kind) {
            Some(dir) => say(&format!("当前登录态已备份到 {}", dir.display())),
            None => say("未检测到可备份的当前登录态，跳过备份"),
        }

        // 2) 关闭客户端（不在运行则跳过）
        if is_running(kind) {
            say(&format!("正在关闭 {}…", kind.display_name()));
            close_client(kind, 30)?;
            say("客户端已关闭");
        }

        // 3) 恢复快照
        say(&format!("正在恢复账号 {name} 的登录态…"));
        let restored = snapshot::restore_account(&acc)?;
        say(&format!("登录态已恢复（{restored} 组文件 + agent token）"));

        // 4) 启动
        if restart {
            launch_client(kind, Some(&|msg: &str| say(msg)))?;
        }
        Ok(restored)
    })();

    let result = outcome;
    if let Err(e) = &result {
        say(&format!("切换失败: {e}"));
    }
    progress_finish();
    let _restored = result?;
    Ok(json!({
        "ok": true,
        "account": account::account_meta(&acc),
        "client": kind.as_str(),
        "restarted": restart,
        "alreadyCurrent": false,
    }))
}

/// 客户端状态汇总（状态页）。
pub fn client_status(kind: ClientKind) -> Value {
    let app_path = crate::modules::process::installed_app_path(kind);
    let identity = snapshot::current_identity(kind);
    json!({
        "client": kind.as_str(),
        "clientName": kind.display_name(),
        "installed": app_path.is_some(),
        "appPath": app_path.map(|p| p.to_string_lossy().to_string()),
        "running": is_running(kind),
        "loggedIn": identity.is_some(),
        "currentUid": identity.as_ref().map(|i| i.user_id.clone()),
        "expiresAt": identity.as_ref().and_then(|i| i.expires_at),
    })
}
