/**
 * 主题偏好：system / light / dark。
 * 借鉴 workbuddy-switch，三态选择，localStorage 持久化，系统主题变化时自动跟随。
 */
export type ThemePreference = "system" | "light" | "dark";

const THEME_KEY = "tt-switch.theme";

function isTheme(value: string | null): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

export function getThemePreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(THEME_KEY);
    if (isTheme(stored)) return stored;
  } catch {
    /* 存储不可用时跟随系统 */
  }
  return "system";
}

export function applyTheme(preference: ThemePreference) {
  const dark =
    preference === "dark" ||
    (preference === "system" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

export function setThemePreference(preference: ThemePreference) {
  try {
    localStorage.setItem(THEME_KEY, preference);
  } catch {
    /* 隐私模式写入失败仍立即应用 */
  }
  applyTheme(preference);
}

/** 监听系统主题变化：仅在 preference === "system" 时重新应用。 */
export function watchSystemTheme() {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const onChange = () => {
    if (getThemePreference() === "system") applyTheme("system");
  };
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}
