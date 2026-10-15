//! 进程管理（macOS）：优雅退出 → 超时强杀 → 确认消失；启动后存活校验。
//!
//! 移植自 wb-switch process.rs 的 macOS 部分，目标从 WbVariant 换成 ClientKind。
//! v1 仅支持 macOS；其它平台返回明确错误。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use crate::modules::config;
use crate::modules::variant::ClientKind;

/// 带超时的命令执行（macOS 直接 spawn+wait，子进程极少且生命周期短）。
fn run_cmd_timeout(program: &str, args: &[&str], _timeout_secs: u64) -> Option<Output> {
    Command::new(program).args(args).output().ok()
}

// ---------------------------------------------------------------------------
// macOS：进程枚举
// ---------------------------------------------------------------------------

/// 解析单行 `ps -axo pid=,args=` 输出为 (pid, args)。
#[cfg(target_os = "macos")]
fn parse_ps_row(line: &str) -> Option<(u32, String)> {
    let line = line.trim_start();
    if line.is_empty() {
        return None;
    }
    let split = line.find(|c: char| c.is_whitespace())?;
    let (pid_s, rest) = line.split_at(split);
    let pid = pid_s.trim().parse::<u32>().ok()?;
    let args = rest.trim();
    if args.is_empty() {
        return None;
    }
    Some((pid, args.to_string()))
}

/// 从 `ps` 全量输出中收集命中模式的 (pid, args)。排除自身与 tt-switch 进程。
#[cfg(target_os = "macos")]
fn filter_ps_rows(stdout: &str, patterns: &[String], self_pid: u32) -> Vec<(u32, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Some((pid, args)) = parse_ps_row(line) else {
            continue;
        };
        if pid == self_pid {
            continue;
        }
        if args.contains("tt-switch") || args.contains("TT Switch") {
            continue;
        }
        if !patterns.iter().any(|p| args.contains(p.as_str())) {
            continue;
        }
        if seen.insert(pid) {
            out.push((pid, args));
        }
    }
    out
}

/// 主进程层匹配模式：解析到 app 路径时用 `<app>/Contents/MacOS`。
#[cfg(target_os = "macos")]
fn macos_main_patterns(resolved_app: Option<&Path>, kind: ClientKind) -> Vec<String> {
    match resolved_app {
        Some(app) => vec![format!("{}/Contents/MacOS", app.display())],
        None => kind
            .macos_app_names()
            .iter()
            .map(|name| format!("{name}/Contents/MacOS"))
            .collect(),
    }
}

/// 包内层匹配模式：解析到 app 路径时用其完整路径；否则回退字面量（含全小写变体）。
#[cfg(target_os = "macos")]
fn macos_bundle_patterns(resolved_app: Option<&Path>, kind: ClientKind) -> Vec<String> {
    match resolved_app {
        Some(app) => vec![app.display().to_string()],
        None => kind
            .macos_app_names()
            .iter()
            .flat_map(|name| [(*name).to_string(), name.to_lowercase()])
            .collect(),
    }
}

#[cfg(target_os = "macos")]
fn ps_all_rows() -> String {
    match run_cmd_timeout("ps", &["-axo", "pid=,args="], 5) {
        Some(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        None => String::new(),
    }
}

#[cfg(target_os = "macos")]
fn macos_rows_by_patterns(patterns: &[String]) -> Vec<(u32, String)> {
    filter_ps_rows(&ps_all_rows(), patterns, std::process::id())
}

#[cfg(target_os = "macos")]
fn macos_pids_by_patterns(patterns: &[String]) -> Vec<u32> {
    macos_rows_by_patterns(patterns)
        .into_iter()
        .map(|(pid, _)| pid)
        .collect()
}

/// 从主进程 argv 提取 `.app` bundle 路径。
#[cfg(target_os = "macos")]
fn extract_app_bundle_from_args(args: &str) -> Option<PathBuf> {
    let idx = args.find(".app/Contents/MacOS")?;
    let path = &args[..idx + 4];
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

/// macOS app bundle 谓词：目录存在且含 Contents/Info.plist。
#[cfg(target_os = "macos")]
pub fn is_app_bundle(path: &Path) -> bool {
    path.is_dir() && path.join("Contents").join("Info.plist").is_file()
}

/// 运行中主进程路径探测。
#[cfg(target_os = "macos")]
fn macos_running_app_path(kind: ClientKind) -> Option<PathBuf> {
    let patterns = macos_main_patterns(None, kind);
    for (_pid, args) in macos_rows_by_patterns(&patterns) {
        if let Some(p) = extract_app_bundle_from_args(&args) {
            if is_app_bundle(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// mdfind 按 bundle id 探测（Spotlight 不可用则静默跳过）。
#[cfg(target_os = "macos")]
fn macos_mdfind_app_path(kind: ClientKind) -> Option<PathBuf> {
    let query = format!("kMDItemCFBundleIdentifier == '{}'c", kind.macos_bundle_id());
    let out = run_cmd_timeout("mdfind", &[query.as_str()], 5)?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
}

/// 命中即写缓存；已是同一路径则不重复写。
#[cfg(target_os = "macos")]
fn persist_app_cache(path: &Path, kind: ClientKind) {
    if !is_app_bundle(path) {
        return;
    }
    if config::load_app_cache(kind).as_deref() == Some(path) {
        return;
    }
    let _ = config::save_app_cache(kind, path);
}

/// app 路径动态探测：运行中主进程 → 缓存 → 常见位置 → mdfind bundle id。
#[cfg(target_os = "macos")]
fn macos_app_path_resolved(kind: ClientKind) -> Option<PathBuf> {
    if let Some(p) = macos_running_app_path(kind) {
        persist_app_cache(&p, kind);
        return Some(p);
    }
    if let Some(cached) = config::load_app_cache(kind) {
        if is_app_bundle(&cached) {
            return Some(cached);
        }
        config::clear_app_cache(kind);
    }
    let home_apps = config::home_dir().join("Applications");
    for base in [Path::new("/Applications"), home_apps.as_path()] {
        for name in kind.macos_app_names() {
            let p = base.join(name);
            if is_app_bundle(&p) {
                persist_app_cache(&p, kind);
                return Some(p);
            }
        }
    }
    if let Some(p) = macos_mdfind_app_path(kind) {
        if is_app_bundle(&p) {
            persist_app_cache(&p, kind);
            return Some(p);
        }
    }
    None
}

/// 解析 app 路径；全部失败回退默认路径（供启动失败文案使用）。
#[cfg(target_os = "macos")]
pub fn macos_app_path(kind: ClientKind) -> PathBuf {
    macos_app_path_resolved(kind).unwrap_or_else(|| kind.macos_default_app_path())
}

#[cfg(target_os = "macos")]
fn kill_macos_pids(pids: &[u32]) {
    if pids.is_empty() {
        return;
    }
    let owned: Vec<String> = std::iter::once("-9".to_string())
        .chain(pids.iter().map(|pid| pid.to_string()))
        .collect();
    let args: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    let _ = run_cmd_timeout("kill", &args, 10);
}

/// 轮询等待「按模式命中」的进程集合为空；超时返回仍存活的 pid。
#[cfg(target_os = "macos")]
fn wait_macos_patterns_empty(patterns: &[String], timeout: Duration) -> Vec<u32> {
    if patterns.is_empty() {
        return Vec::new();
    }
    let deadline = Instant::now() + timeout;
    loop {
        let alive = macos_pids_by_patterns(patterns);
        if alive.is_empty() || Instant::now() >= deadline {
            return alive;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

// ---------------------------------------------------------------------------
// 对外 API
// ---------------------------------------------------------------------------

/// 客户端主进程是否在运行。
pub fn is_running(kind: ClientKind) -> bool {
    #[cfg(target_os = "macos")]
    {
        let resolved = macos_app_path_resolved(kind);
        let patterns = macos_main_patterns(resolved.as_deref(), kind);
        !macos_pids_by_patterns(&patterns).is_empty()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        false
    }
}

/// 已安装的 app 路径（未安装返回 None）。
pub fn installed_app_path(kind: ClientKind) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos_app_path_resolved(kind)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        None
    }
}

/// 关闭客户端：osascript 优雅退出 → 优雅窗口等待 → 清杀包内残留 → 确认消失。
pub fn close_client(kind: ClientKind, timeout_secs: i64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let started = Instant::now();
        let timeout = Duration::from_secs(timeout_secs.max(1) as u64);
        let resolved = macos_app_path_resolved(kind);
        let main_patterns = macos_main_patterns(resolved.as_deref(), kind);
        let bundle_patterns = macos_bundle_patterns(resolved.as_deref(), kind);
        let remaining = || {
            timeout
                .saturating_sub(started.elapsed())
                .max(Duration::from_millis(100))
        };

        // 1) 优雅退出
        let quit_script = format!("quit app id \"{}\"", kind.macos_bundle_id());
        let quit = run_cmd_timeout("osascript", &["-e", quit_script.as_str()], 10);
        match quit {
            Some(out) if !out.status.success() => {
                eprintln!(
                    "[close] osascript quit failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            None => eprintln!("[close] osascript quit timed out"),
            _ => {}
        }

        // 2) 优雅窗口等待主进程消失
        let graceful = Duration::from_secs(8).min(remaining());
        if !wait_macos_patterns_empty(&main_patterns, graceful).is_empty() {
            eprintln!("[close] graceful quit not effective, forcing bundle kill…");
        }

        // 3) 清杀包内残留（主进程 + 守护子进程）
        let bundle_pids = macos_pids_by_patterns(&bundle_patterns);
        if !bundle_pids.is_empty() {
            kill_macos_pids(&bundle_pids);
        }

        // 4) 等包内集合为空
        let leftover = wait_macos_patterns_empty(&bundle_patterns, remaining());
        if leftover.is_empty() {
            return Ok(());
        }
        let pids: Vec<String> = leftover.iter().map(|pid| pid.to_string()).collect();
        return Err(format!(
            "{} 进程无法完全关闭（残留进程: {}）。请手动执行: kill -9 {}",
            kind.display_name(),
            pids.join(", "),
            pids.join(" ")
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (kind, timeout_secs);
        Err("当前平台暂不支持关闭客户端（v1 仅支持 macOS）".to_string())
    }
}

/// 启动存活校验：主进程出现且持续存活 ≥10s；出现后秒退判单例锁残留。
#[cfg(target_os = "macos")]
fn validate_macos_startup(
    app: &Path,
    kind: ClientKind,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), String> {
    let say = |msg: &str| {
        if let Some(p) = progress {
            p(msg);
        }
    };
    let main_patterns = macos_main_patterns(Some(app), kind);
    let deadline = Instant::now() + Duration::from_secs(30);
    let sustain = Duration::from_secs(10);
    let mut seen_at: Option<Instant> = None;
    while Instant::now() < deadline {
        let now = Instant::now();
        let alive = !macos_pids_by_patterns(&main_patterns).is_empty();
        if alive {
            match seen_at {
                None => {
                    seen_at = Some(now);
                    say(&format!("{} 窗口已出现，正在确认稳定运行…", kind.display_name()));
                }
                Some(start) => {
                    if now.duration_since(start) >= sustain {
                        return Ok(());
                    }
                }
            }
        } else if seen_at.is_some() {
            return Err(format!(
                "{} 启动后立即退出，疑似残留单例锁。请手动打开一次后再试。",
                kind.display_name()
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "{} 启动超时，未能确认运行（路径: {}）。请手动打开排查。",
        kind.display_name(),
        app.display()
    ))
}

/// 启动客户端：路径检查 → 保险清杀 → open -n -a → 存活校验。
pub fn launch_client(kind: ClientKind, progress: Option<&dyn Fn(&str)>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let say = |msg: &str| {
            if let Some(p) = progress {
                p(msg);
            }
        };
        let app = macos_app_path(kind);
        if !is_app_bundle(&app) {
            return Err(format!(
                "未找到 {}（尝试路径: {}）。请先安装或手动打开一次后重试。",
                kind.display_name(),
                app.display()
            ));
        }

        // 保险清杀：包内残留（如持锁守护）非空则 kill -9 全部并等到空（5s）。
        let bundle_patterns = macos_bundle_patterns(Some(&app), kind);
        let bundle_pids = macos_pids_by_patterns(&bundle_patterns);
        if !bundle_pids.is_empty() {
            say("正在清理残留进程…");
            kill_macos_pids(&bundle_pids);
            let _ = wait_macos_patterns_empty(&bundle_patterns, Duration::from_secs(5));
        }

        say(&format!("正在启动 {}…", kind.display_name()));
        let app_lossy = app.to_string_lossy();
        let open = run_cmd_timeout("open", &["-n", "-a", app_lossy.as_ref()], 10);
        match open {
            Some(out) if out.status.success() => {}
            Some(out) => {
                let reason = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let reason = if reason.is_empty() {
                    format!("open 退出码 {}", out.status.code().unwrap_or(-1))
                } else {
                    reason
                };
                return Err(format!(
                    "启动 {} 失败: {reason}（路径: {}）",
                    kind.display_name(),
                    app.display()
                ));
            }
            None => {
                return Err(format!(
                    "启动 {} 失败: open 超时（路径: {}）",
                    kind.display_name(),
                    app.display()
                ));
            }
        }

        validate_macos_startup(&app, kind, progress)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (kind, progress);
        Err("当前平台暂不支持启动客户端（v1 仅支持 macOS）".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_ps_row_basic() {
        assert_eq!(
            parse_ps_row("  123 /Applications/Trae CN.app/Contents/MacOS/Trae CN"),
            Some((123, "/Applications/Trae CN.app/Contents/MacOS/Trae CN".to_string()))
        );
        assert_eq!(parse_ps_row(""), None);
        assert_eq!(parse_ps_row("abc def"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn extract_bundle_from_args() {
        assert_eq!(
            extract_app_bundle_from_args("/Applications/Trae CN.app/Contents/MacOS/Trae CN --flag"),
            Some(PathBuf::from("/Applications/Trae CN.app"))
        );
        assert_eq!(extract_app_bundle_from_args("/usr/bin/ls"), None);
    }
}
