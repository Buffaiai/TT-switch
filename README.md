# TT Switch

Trae 多账号切换工具（Rust / Tauri 版）。

在一台机器上管理多个 Trae 账号：采集当前登录态为快照，随时一键切换到任意账号；同一份代码同时提供 **Tauri 桌面应用** 与 **浏览器 webui** 两种形态。

## 功能

- **账号快照与一键切换**：采集 Trae（IDE / SOLO，国内版 / 国际版）当前登录态，切换时自动备份、关闭并重启对应客户端；按 uid 归并展示，同一身份一张卡片
- **自动签到**：每天自动完成 Trae 签到积分领取（后台 60s 定时器，失败当日延迟重试），卡片上实时显示积分与签到状态
- **积分 / Token 统计**：积分统计页、Token 用量统计页
- **迁移**：导出 / 导入账号迁移包（含路径穿越、符号链接与绝对路径防护）
- **双形态**：Tauri 2 桌面端（系统托盘常驻）+ `tt-switch` CLI 启动的本地 webui（`http://127.0.0.1:57990`），同一套 React 前端

## 技术栈

- **后端**：Rust（workspace）— `tt-switch-core`（全部本地逻辑）、`tt-switch-server`（axum HTTP 服务 + CLI）
- **桌面端**：Tauri 2（`src-tauri`）
- **前端**：React 19 + Vite + Tailwind CSS 4 + shadcn/ui + Zustand，双通道 API（浏览器走 HTTP fetch，桌面端走 Tauri IPC）

## 目录结构

```
├── src/                  # React 前端（页面、组件、stores）
├── src-tauri/            # Tauri 2 桌面端（命令注册、托盘、后台定时器）
├── crates/
│   ├── tt-switch-core/   # 核心逻辑：账号/快照/切换/签到/迁移/系统探测
│   └── tt-switch-server/ # axum HTTP 服务 + tt-switch CLI
├── index.html
└── vite.config.ts
```

## 开发

前置要求：Node.js、Rust 工具链、Tauri 2 系统依赖。

```bash
npm install

# 桌面端开发预览
npm run tauri dev

# 浏览器 webui（构建前端后启动本地服务，自动打开浏览器）
npm run build
cargo run -p tt-switch-server -- serve
```

## 构建

```bash
# 前端产物
npm run build

# 桌面应用
npm run tauri build

# CLI / webui 服务（产物为 target/release/tt-switch）
cargo build --release -p tt-switch-server
```

## CLI 用法

```bash
tt-switch              # 启动本地服务并打开浏览器 webui
tt-switch serve        # 只起服务（--port <端口> / --no-open）
tt-switch status       # 终端输出各客户端安装/登录状态与账号数
tt-switch version      # 版本号
```

## 测试

```bash
cargo test          # core 单元测试
npx tsc --noEmit    # 前端类型检查
```

## 说明

- 账号数据仅保存在本机，不上传任何服务器
- 快照采集使用 SQLite Online Backup API 只读访问，保证 IDE 运行中的事务一致性
- 每个客户端最多保留最近 10 份自动备份，超出自动清理
