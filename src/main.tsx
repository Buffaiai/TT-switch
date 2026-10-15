import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/bricolage-grotesque";
import App from "./App";
import "./index.css";
import { desktopApi, isWebui } from "./lib/api";
import { applyTheme, getThemePreference, watchSystemTheme } from "./lib/theme";

// 渲染前应用主题，避免首屏闪白/闪黑。
applyTheme(getThemePreference());
watchSystemTheme();

// 桌面宿主下，把界面未捕获错误写入本机错误日志（webui 无本机通道，静默）。
if (!isWebui()) {
  const report = (message: string) => {
    try {
      void desktopApi.logFrontendError(message);
    } catch {
      /* 日志失败不打扰用户 */
    }
  };
  window.addEventListener("error", (event) => {
    report(`uncaught: ${event.message} @ ${event.filename}:${event.lineno}`);
  });
  window.addEventListener("unhandledrejection", (event) => {
    const reason =
      event.reason instanceof Error ? event.reason.message : String(event.reason);
    report(`unhandledrejection: ${reason}`);
  });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
