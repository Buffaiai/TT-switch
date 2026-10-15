/**
 * 用户界面偏好：客户端可见性开关 + 紧凑模式。
 * 借鉴 workbuddy-switch 的 supported-tools 模式：localStorage 持久化，
 * 同标签页通过自定义事件同步。
 */
import { useEffect } from "react";
import type { ClientKind } from "./types";

const VISIBILITY_PREFIX = "tt-switch.client.";
const COMPACT_KEY = "tt-switch.compact";
const CHANGED_EVENT = "tt-switch:prefs-changed";

/** 各客户端默认可见性：已安装的默认显示，未安装的默认隐藏。 */
export function defaultVisibility(installed: boolean): boolean {
  return installed;
}

export function isClientVisible(client: ClientKind, fallback: boolean): boolean {
  try {
    const stored = localStorage.getItem(VISIBILITY_PREFIX + client);
    if (stored === null) return fallback;
    return stored === "1";
  } catch {
    return fallback;
  }
}

export function setClientVisible(client: ClientKind, visible: boolean) {
  try {
    localStorage.setItem(VISIBILITY_PREFIX + client, visible ? "1" : "0");
  } catch {
    /* 写入失败不打扰 */
  }
  window.dispatchEvent(new Event(CHANGED_EVENT));
}

export function getCompactMode(): boolean {
  try {
    return localStorage.getItem(COMPACT_KEY) === "1";
  } catch {
    return false;
  }
}

export function setCompactMode(enabled: boolean) {
  try {
    localStorage.setItem(COMPACT_KEY, enabled ? "1" : "0");
  } catch {
    /* 忽略 */
  }
  window.dispatchEvent(new Event(CHANGED_EVENT));
}

/** 订阅偏好变化（同标签页内的开关改动即时同步）。 */
export function usePrefsSync(onChange: () => void) {
  useEffect(() => {
    window.addEventListener(CHANGED_EVENT, onChange);
    window.addEventListener("storage", onChange);
    return () => {
      window.removeEventListener(CHANGED_EVENT, onChange);
      window.removeEventListener("storage", onChange);
    };
  }, [onChange]);
}
