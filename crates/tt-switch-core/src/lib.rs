//! tt-switch 核心库：Trae 多账号切换的全部本地逻辑。
//!
//! 不依赖 Tauri / axum，桌面端（src-tauri）与 HTTP server（tt-switch-server）
//! 共用这里的模块。凭据文件一律整文件不透明拷贝，本库不读取、不打印其内容。

pub mod modules;
