//! tt-switch CLI：
//!
//! ```bash
//! tt-switch              # 启动本地服务 + 打开浏览器 webui
//! tt-switch serve        # 只起服务不开浏览器（--port / --no-open）
//! tt-switch status       # 终端输出当前状态
//! tt-switch version      # 版本号
//! ```

mod api;

use tt_switch_core::modules::{account, switch, variant::ClientKind};

fn default_port() -> u16 {
    57990
}

fn print_status() {
    println!("tt-switch v{}", env!("CARGO_PKG_VERSION"));
    for kind in ClientKind::ALL {
        let status = switch::client_status(kind);
        let running = status.get("running").and_then(|v| v.as_bool()) == Some(true);
        let installed = status.get("installed").and_then(|v| v.as_bool()) == Some(true);
        let uid = status
            .get("currentUid")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        println!(
            "{}: {} | 运行中: {} | 当前 uid: {}",
            kind.display_name(),
            if installed { "已安装" } else { "未安装" },
            if running { "是" } else { "否" },
            uid
        );
    }
    println!("账号数: {}", account::load_accounts().len());
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("serve");
    match cmd {
        "status" => print_status(),
        "version" | "--version" | "-V" => {
            println!("tt-switch {}", env!("CARGO_PKG_VERSION"));
        }
        _ => serve(&args).await,
    }
}

async fn serve(args: &[String]) {
    let mut port = default_port();
    if let Some(i) = args.iter().position(|a| a == "--port") {
        if let Some(p) = args.get(i + 1).and_then(|p| p.parse::<u16>().ok()) {
            port = p;
        }
    }

    let app = api::router();
    let addr = format!("127.0.0.1:{port}");
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("启动失败: 端口 {port} 被占用或不可用（{e}）。可用 --port 指定其他端口。");
            std::process::exit(1);
        }
    };

    println!("tt-switch v{}", env!("CARGO_PKG_VERSION"));
    println!("webui: http://{addr}");
    println!("按 Ctrl+C 停止服务。");

    if !args.iter().any(|a| a == "--no-open") {
        open_browser(&addr);
    }

    axum::serve(listener, app).await.unwrap();
}

fn open_browser(addr: &str) {
    let url = format!("http://{addr}");
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", &url])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
}
