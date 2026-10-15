//! Tauri commands：core 能力的薄封装。与 HTTP server 的 api.rs 一一对应。
//!
//! 可能执行子进程（ps/osascript/open）的命令一律 async + spawn_blocking，
//! 避免阻塞窗口消息循环。

use serde_json::{json, Value};
use tauri_plugin_notification::NotificationExt;
use tt_switch_core::modules::{account, checkin, commercial, error_log, switch, system, transfer, variant::ClientKind};

fn parse_client(raw: Option<String>) -> Result<ClientKind, String> {
    ClientKind::parse(raw.as_deref())
        .ok_or_else(|| "缺少有效 client 参数（ide_cn / solo_cn / ide_intl）".to_string())
}

/// 三客户端状态 + 账号数。
#[tauri::command]
pub async fn get_status() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let clients: Vec<Value> = ClientKind::ALL.iter().map(|k| switch::client_status(*k)).collect();
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "clients": clients,
            "accountCount": account::load_accounts().len(),
        })
    })
    .await
    .map_err(|e| format!("查询状态失败: {e}"))
}

/// 账号列表（元数据，不含凭据）。
#[tauri::command]
pub fn get_accounts() -> Value {
    let metas: Vec<Value> = account::load_accounts()
        .iter()
        .map(account::account_meta)
        .collect();
    json!({ "accounts": metas })
}

/// 采集当前登录态为账号。
#[tauri::command]
pub async fn capture_account(client: String) -> Result<Value, String> {
    let kind = parse_client(Some(client))?;
    tauri::async_runtime::spawn_blocking(move || {
        switch::capture_current(kind)
            .map(|record| json!({ "ok": true, "account": account::account_meta(&record) }))
            .map_err(|e| {
                error_log::log_error("capture", &e);
                e
            })
    })
    .await
    .map_err(|e| format!("采集失败: {e}"))?
}

/// 更新标注名。
#[tauri::command]
pub fn rename_account(id: String, nickname: String) -> Result<Value, String> {
    account::rename_account(&id, &nickname)?;
    Ok(json!({ "ok": true }))
}

/// 删除账号（记录 + 快照目录）。
#[tauri::command]
pub fn delete_account(id: String) -> Result<Value, String> {
    account::delete_account(&id)?;
    tt_switch_core::modules::snapshot::remove_snapshot(&id);
    Ok(json!({ "ok": true }))
}

/// 切换账号：关客户端 → 恢复快照 → 重启；完成后发系统通知。
#[tauri::command]
pub async fn switch_account(
    app: tauri::AppHandle,
    id: String,
    restart: Option<bool>,
) -> Result<Value, String> {
    if id.trim().is_empty() {
        return Err("缺少 id".to_string());
    }
    let restart = restart.unwrap_or(true);
    let result = tauri::async_runtime::spawn_blocking(move || {
        switch::switch_account(&id, restart, None).map_err(|e| {
            error_log::log_error("switch", &e);
            e
        })
    })
    .await
    .map_err(|e| format!("切换失败: {e}"))??;

    // 后台/托盘可见性：系统通知播报切换结果。
    let name = result
        .get("account")
        .and_then(|a| a.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("目标账号")
        .to_string();
    let body = if result.get("alreadyCurrent").and_then(Value::as_bool) == Some(true) {
        format!("当前已是「{name}」")
    } else {
        format!("已切换到「{name}」")
    };
    let _ = app
        .notification()
        .builder()
        .title("TT Switch")
        .body(&body)
        .show();
    Ok(result)
}

/// 切换进度（前端轮询）。
#[tauri::command]
pub fn switch_progress() -> Value {
    switch::progress_snapshot()
}

/// 错误日志。
#[tauri::command]
pub fn get_error_logs() -> Value {
    json!({
        "path": error_log::error_log_path(),
        "logs": error_log::load_error_logs(),
    })
}

/// 存储路径 + 备份清单。
#[tauri::command]
pub fn get_system_info() -> Value {
    let mut info = system::storage_paths();
    if let Some(obj) = info.as_object_mut() {
        obj.insert("backups".to_string(), json!(system::list_backups()));
    }
    info
}

/// 数据目录可写性自检。
#[tauri::command]
pub async fn check_permission(client: String) -> Result<Value, String> {
    let kind = parse_client(Some(client))?;
    tauri::async_runtime::spawn_blocking(move || system::check_data_dir_permission(kind))
        .await
        .map_err(|e| format!("权限自检失败: {e}"))
}

/// 打开系统隐私设置面板。
#[tauri::command]
pub fn open_privacy_settings(target: Option<String>) -> Result<(), String> {
    system::open_privacy_settings(target.as_deref().unwrap_or("all_files"))
}

/// Finder 定位：store | backups | clientData | clientApp。
#[tauri::command]
pub fn reveal(target: String, client: Option<String>) -> Result<(), String> {
    match target.as_str() {
        "store" => system::reveal_path(&tt_switch_core::modules::config::store_dir()),
        "backups" => {
            let dir = tt_switch_core::modules::config::backup_dir();
            let _ = std::fs::create_dir_all(&dir);
            system::reveal_path(&dir)
        }
        "errorLog" => system::reveal_path(&tt_switch_core::modules::config::error_log_file()),
        "clientData" | "clientApp" => {
            let kind = parse_client(client)?;
            if target == "clientApp" {
                system::reveal_client_app(kind)
            } else {
                system::reveal_path(&kind.data_root())
            }
        }
        _ => Err("未知 target".to_string()),
    }
}

/// 删除单份安全备份。
#[tauri::command]
pub fn delete_backup(name: String) -> Result<(), String> {
    let valid_prefix = ClientKind::ALL
        .iter()
        .any(|k| name.starts_with(&format!("{}-", k.as_str())));
    if !valid_prefix || name.contains('/') || name.contains("..") {
        return Err("非法备份名".to_string());
    }
    let path = tt_switch_core::modules::config::backup_dir().join(&name);
    if !path.is_dir() {
        return Err("备份不存在".to_string());
    }
    std::fs::remove_dir_all(&path).map_err(|e| format!("删除失败: {e}"))
}

/// 重启 App（授权系统权限后生效用）。
#[tauri::command]
pub fn relaunch_app(app: tauri::AppHandle) {
    app.restart();
}

/// 开机自启状态。
#[tauri::command]
pub fn get_launch_at_login_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// 设置开机自启（注册时携带 --hidden，启动后静默驻留托盘）。
#[tauri::command]
pub fn set_launch_at_login_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())
    } else {
        autolaunch.disable().map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// 账号迁移（导出/导入 tar 包）
// ---------------------------------------------------------------------------

/// 弹保存框导出账号；ids 为空导出全部。用户取消返回 Ok(None)。
#[tauri::command]
pub async fn transfer_export_dialog(
    app: tauri::AppHandle,
    ids: Option<Vec<String>>,
) -> Result<Option<Value>, String> {
    use tauri_plugin_dialog::DialogExt;
    let default_name = format!(
        "tt-switch-export-{}.tar",
        chrono::Local::now().format("%Y%m%d")
    );
    let target = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("TT Switch 迁移包", &["tar"])
            .set_file_name(&default_name)
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("保存对话框失败: {e}"))?;
    let Some(path) = target else { return Ok(None) };
    let path = path.to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        transfer::export_accounts(&ids.unwrap_or_default(), &path)
    })
    .await
    .map_err(|e| format!("导出失败: {e}"))??;
    Ok(Some(result))
}

/// 弹打开框选择迁移包并返回导入预览；取消返回 Ok(None)。
#[tauri::command]
pub async fn transfer_preview_dialog(app: tauri::AppHandle) -> Result<Option<Value>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("TT Switch 迁移包", &["tar"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("打开对话框失败: {e}"))?;
    let Some(path) = picked else { return Ok(None) };
    let path = path.to_string();
    let preview = tauri::async_runtime::spawn_blocking(move || transfer::preview_import(&path))
        .await
        .map_err(|e| format!("预览失败: {e}"))??;
    Ok(Some(preview))
}

/// 按预览下标执行导入（路径来自上一步选择，不弹框）。
#[tauri::command]
pub async fn transfer_import(path: String, indexes: Vec<usize>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || transfer::import_accounts(&path, &indexes))
        .await
        .map_err(|e| format!("导入失败: {e}"))?
}

/// 积分 / Token 商业数据（缓存 + 云端）。
#[tauri::command]
pub async fn get_commercial_stats(account_id: Option<String>) -> Result<Value, String> {
    Ok(commercial::get_stats(account_id.as_deref()).await)
}

/// 本地全账号概览（零网络请求）：悬浮窗与跨账号对比图用。
#[tauri::command]
pub fn get_commercial_local_overview() -> Result<Value, String> {
    Ok(commercial::local_overview())
}

/// 每日签到：配置 + 每账号最近结果（前端徽章 / 签到卡片）。
#[tauri::command]
pub fn get_checkin_status() -> Result<Value, String> {
    Ok(checkin::status_summary())
}

/// 每日签到：更新开关 / 时间（HH:MM），返回最新状态。
#[tauri::command]
pub fn set_checkin_config(enabled: Option<bool>, time: Option<String>) -> Result<Value, String> {
    checkin::set_config(enabled, time.as_deref())
}

/// 每日签到：立即手动触发全账号签到（同日幂等仍生效）。
#[tauri::command]
pub async fn run_checkin_now() -> Result<Value, String> {
    Ok(checkin::run_all(true).await)
}

/// 打开 Companion 悬浮窗（置顶小窗：当前账号 + 快捷切换）。
/// 已存在则显示并聚焦；关闭后再次调用会重建。
#[tauri::command]
pub fn open_companion(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("companion") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "companion",
        tauri::WebviewUrl::App("index.html#/companion".into()),
    )
    .title("TT Switch 悬浮窗")
    .inner_size(320.0, 480.0)
    .min_inner_size(300.0, 400.0)
    .decorations(false)
    .always_on_top(true)
    .resizable(true)
    .skip_taskbar(true)
    .shadow(true)
    .build()
    .map_err(|e| format!("创建悬浮窗失败: {e}"))?;
    Ok(())
}

/// 显示并聚焦主窗口（悬浮窗 / 托盘共用）。
#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) {
    crate::tray::show_main_window(&app);
}

/// 前端未捕获错误写本机错误日志（与 core 错误同一文件）。
#[tauri::command]
pub fn log_frontend_error(message: String) {
    let trimmed: String = message.chars().take(2000).collect();
    error_log::log_error("frontend", &trimmed);
}
