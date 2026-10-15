mod commands;
#[cfg(desktop)]
mod tray;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // 单实例互斥必须最先注册：第二个进程在建窗口之前退出，
    // 不会产生账号侧副作用。
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }));
    }

    builder = builder
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_notification::init())
            .plugin(tauri_plugin_dialog::init());

    // 开机自启：注册为 LaunchAgent，携带 --hidden 实现静默驻留托盘。
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![tray::SILENT_STARTUP_ARG]),
        ));
        builder = builder.on_window_event(tray::on_window_event);
    }

    let app = builder
        .setup(|app| {
            #[cfg(desktop)]
            {
                tray::setup(app)?;
                // 仅系统自启（精确 --hidden 参数）进入静默托盘；普通启动正常显示。
                if tray::is_silent_startup(std::env::args()) {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
            // 每日自动签到：常驻后台定时器（App 托盘驻留期间生效，窗口关闭不影响）。
            // tick 内部读取 checkin.json 判定 enabled/到点/同日幂等，此处只做周期驱动。
            tauri::async_runtime::spawn(async {
                let mut interval =
                    tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    tt_switch_core::modules::checkin::scheduler_tick().await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_accounts,
            commands::capture_account,
            commands::rename_account,
            commands::delete_account,
            commands::switch_account,
            commands::switch_progress,
            commands::get_error_logs,
            commands::get_system_info,
            commands::check_permission,
            commands::open_privacy_settings,
            commands::reveal,
            commands::delete_backup,
            commands::relaunch_app,
            commands::get_launch_at_login_enabled,
            commands::set_launch_at_login_enabled,
            commands::transfer_export_dialog,
            commands::transfer_preview_dialog,
            commands::transfer_import,
            commands::get_commercial_stats,
            commands::get_commercial_local_overview,
            commands::get_checkin_status,
            commands::set_checkin_config,
            commands::run_checkin_now,
            commands::open_companion,
            commands::show_main_window,
            commands::log_frontend_error,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        // Dock / Finder 再次激活：从托盘恢复主窗口。
        #[cfg(desktop)]
        tray::on_run_event(app_handle, &event);
    });
}
