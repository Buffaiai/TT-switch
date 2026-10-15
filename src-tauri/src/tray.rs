//! 系统托盘：菜单栏图标 + 驻留模式（关窗不退出）+ Dock 重开唤起。

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent, Runtime, Window, WindowEvent,
};

const TRAY_ID: &str = "main-menu-bar";
const MAIN_WINDOW: &str = "main";

/// 系统自启注册的启动参数：仅携带该精确参数的启动进入静默托盘模式。
pub const SILENT_STARTUP_ARG: &str = "--hidden";

pub fn setup(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示 TT Switch", true, None::<&str>)?;
    let companion = MenuItem::with_id(app, "companion", "悬浮窗", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 TT Switch", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &companion, &sep, &quit])?;

    // 单色剪影 PNG：macOS 作为 template image 由系统按明暗模式着色。
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
        .unwrap_or_else(|_| app.default_window_icon().cloned().unwrap());

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("TT Switch")
        .menu(&menu)
        // macOS 菜单栏惯例：左键展开菜单。
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_tray_icon_event(|tray, event| {
            // 非 macOS（右键才出菜单）：双击/单击图标唤起主窗口。
            if let TrayIconEvent::DoubleClick { .. } = event {
                show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "companion" => {
                let _ = crate::commands::open_companion(app.clone());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// 显示并聚焦主窗口（托盘菜单 / 第二实例 / Dock Reopen 共用）。
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 关窗拦截：隐藏到托盘而非退出进程（切换器是常驻工具）。
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}

/// Dock / Finder 再次激活已运行实例：重新显示主窗口。
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: &RunEvent) {
    if let RunEvent::Reopen { .. } = event {
        show_main_window(app);
    }
}

/// 本次启动是否携带精确的 `--hidden` 参数（系统自启）。整参相等，禁止子串匹配。
pub fn is_silent_startup(args: impl IntoIterator<Item = impl AsRef<str>>) -> bool {
    args.into_iter().any(|arg| arg.as_ref() == SILENT_STARTUP_ARG)
}
