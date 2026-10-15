import { invoke } from "@tauri-apps/api/core";
import type {
  AccountsPayload,
  AppStatus,
  ErrorLogEntry,
  ExportResult,
  PermissionCheck,
  SwitchProgress,
  SwitchResult,
  SystemInfo,
  TransferPreview,
  TransferResult,
} from "./types";

/**
 * 双通道适配层：
 * - 桌面 App（Tauri）：`invoke` 调用 Rust commands
 * - webui（浏览器）：HTTP fetch 调用本地 tt-switch 服务（127.0.0.1）
 */
const API_BASE = "http://127.0.0.1:57990";

export function isWebui(): boolean {
  return typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window);
}

type Route = { method: "GET" | "POST"; path: string };

/** Tauri command → HTTP 路由映射（webui 模式）。 */
const ROUTES: Record<string, Route> = {
  get_status: { method: "GET", path: "/api/status" },
  get_accounts: { method: "GET", path: "/api/accounts" },
  capture_account: { method: "POST", path: "/api/accounts/capture" },
  rename_account: { method: "POST", path: "/api/accounts/rename" },
  delete_account: { method: "POST", path: "/api/accounts/delete" },
  switch_account: { method: "POST", path: "/api/switch" },
  switch_progress: { method: "GET", path: "/api/switch/progress" },
  get_error_logs: { method: "GET", path: "/api/error-logs" },
  get_system_info: { method: "GET", path: "/api/system/info" },
  check_permission: { method: "POST", path: "/api/system/check-permission" },
  open_privacy_settings: {
    method: "POST",
    path: "/api/system/open-privacy-settings",
  },
  reveal: { method: "POST", path: "/api/system/reveal" },
  delete_backup: { method: "POST", path: "/api/system/delete-backup" },
  get_commercial_stats: { method: "GET", path: "/api/commercial/stats" },
  get_commercial_local_overview: {
    method: "GET",
    path: "/api/commercial/local-overview",
  },
  get_checkin_status: { method: "GET", path: "/api/checkin/status" },
  set_checkin_config: { method: "POST", path: "/api/checkin/config" },
  run_checkin_now: { method: "POST", path: "/api/checkin/run" },
};

async function httpCall<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const route = ROUTES[cmd];
  if (!route) throw new Error(`webui 模式暂不支持该操作: ${cmd}`);
  const url =
    route.method === "GET" && args && Object.keys(args).length
      ? `${API_BASE}${route.path}?${new URLSearchParams(
          Object.entries(args).filter(([, v]) => v !== undefined && v !== null) as [string, string][],
        ).toString()}`
      : `${API_BASE}${route.path}`;
  let res: Response;
  try {
    res = await fetch(url, {
      method: route.method,
      headers: { "Content-Type": "application/json" },
      body: route.method === "POST" ? JSON.stringify(args ?? {}) : undefined,
    });
  } catch {
    throw new Error(`无法连接 tt-switch 服务（${API_BASE}），请先运行 \`tt-switch\``);
  }
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(data.message || data.error || `请求失败 (${res.status})`);
  }
  return data as T;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isWebui()) return httpCall<T>(cmd, args);
  return invoke<T>(cmd, args);
}

export const api = {
  getStatus: () => call<AppStatus>("get_status"),
  getAccounts: () => call<AccountsPayload>("get_accounts"),
  captureAccount: (client: string) =>
    call<{ ok: boolean }>("capture_account", { client }),
  renameAccount: (id: string, nickname: string) =>
    call<{ ok: boolean }>("rename_account", { id, nickname }),
  deleteAccount: (id: string) => call<{ ok: boolean }>("delete_account", { id }),
  switchAccount: (id: string, restart = true) =>
    call<SwitchResult>("switch_account", { id, restart }),
  switchProgress: () => call<SwitchProgress>("switch_progress"),
  getErrorLogs: () =>
    call<{ path: string; logs: ErrorLogEntry[] }>("get_error_logs"),
  getSystemInfo: () => call<SystemInfo>("get_system_info"),
  checkPermission: (client: string) =>
    call<PermissionCheck>("check_permission", { client }),
  openPrivacySettings: (target: "all_files" | "app_management" = "all_files") =>
    call<{ ok: boolean }>("open_privacy_settings", { target }),
  reveal: (target: string, client?: string) =>
    call<{ ok: boolean }>("reveal", { target, client }),
  deleteBackup: (name: string) =>
    call<{ ok: boolean }>("delete_backup", { name }),
  getCommercialStats: (accountId?: string) =>
    call<any>("get_commercial_stats", accountId ? { accountId } : {}),
  getCommercialLocalOverview: () =>
    call<any>("get_commercial_local_overview"),
  getCheckinStatus: () => call<CheckinSummary>("get_checkin_status"),
  setCheckinConfig: (enabled?: boolean, time?: string) =>
    call<CheckinSummary>("set_checkin_config", { enabled, time }),
  runCheckinNow: () => call<CheckinRunResult>("run_checkin_now"),
};

/** 每日签到：每账号最近一次结果。 */
export interface CheckinAccountRec {
  last_claim_date?: string;
  last_result?: "ok" | "already" | "error" | string;
  last_error?: string;
  last_credits?: number | null;
  last_run_ms?: number;
}

/** 每日签到：配置 + 状态汇总（徽章 / 签到卡片数据源）。 */
export interface CheckinSummary {
  ok: boolean;
  today: string;
  enabled: boolean;
  time: string;
  accounts: Record<string, CheckinAccountRec>;
}

/** 每日签到：手动触发返回。 */
export interface CheckinRunResult {
  ok: boolean;
  error?: string;
  today?: string;
  results?: { id: string; name: string; result: string; error?: string; credits?: number | null }[];
}

/** 开机自启 / 迁移 / 错误上报（仅桌面宿主；webui 无此能力）。 */
export const desktopApi = {
  getLaunchAtLogin: () =>
    invoke<boolean>("get_launch_at_login_enabled"),
  setLaunchAtLogin: (enabled: boolean) =>
    invoke<void>("set_launch_at_login_enabled", { enabled }),
  relaunch: () => invoke<void>("relaunch_app"),
  exportAccounts: (ids: string[]) =>
    invoke<ExportResult | null>("transfer_export_dialog", { ids }),
  previewImport: () =>
    invoke<TransferPreview | null>("transfer_preview_dialog"),
  importAccounts: (path: string, indexes: number[]) =>
    invoke<TransferResult>("transfer_import", { path, indexes }),
  logFrontendError: (message: string) =>
    invoke<void>("log_frontend_error", { message }),
  openCompanion: () => invoke<void>("open_companion"),
  showMainWindow: () => invoke<void>("show_main_window"),
};
