import { HashRouter, Navigate, NavLink, Outlet, Route, Routes } from "react-router-dom";
import { Link2, MessagesSquare, Settings, Sparkles, User } from "lucide-react";

import { AppIconMark, StatusDot } from "@/components/product-marks";
import { Toaster } from "@/components/ui/sonner";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { isWebui } from "@/lib/api";
import { cn } from "@/lib/utils";
import AccountsPage from "@/pages/AccountsPage";
import CompanionPage from "@/pages/CompanionPage";
import CreditStatsPage from "@/pages/CreditStatsPage";
import SessionsPage from "@/pages/SessionsPage";
import SettingsPage from "@/pages/SettingsPage";
import TokenStatsPage from "@/pages/TokenStatsPage";
import { useAccountsStore } from "@/stores/accounts";

const NAV_ITEMS = [
  { to: "/", label: "账号管理", icon: User, end: true },
  { to: "/sessions", label: "关联会话", icon: Link2 },
  { to: "/token-stats", label: "Token 统计", icon: MessagesSquare },
  { to: "/credit-stats", label: "积分统计", icon: Sparkles },
  { to: "/settings", label: "设置", icon: Settings },
] as const;

function navClass({ isActive }: { isActive: boolean }) {
  return cn(
    "flex items-center gap-2.5 rounded-lg px-3 py-2.5 text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-sidebar-ring/50",
    isActive
      ? "bg-foreground/[0.06] font-medium text-foreground"
      : "text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground",
  );
}

/** 侧边栏底部：客户端运行状态 + 版本号（更新按钮随自动更新能力接入）。 */
function SidebarFooter() {
  const status = useAccountsStore((s) => s.status);
  const running = status?.clients.some((c) => c.running);
  const version = status?.version;

  return (
    <section className="mt-auto border-t border-sidebar-border px-2 pt-3 text-xs">
      <div className="flex items-center gap-2 text-[13px] text-sidebar-foreground">
        <StatusDot on={Boolean(running)} />
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="min-w-0 flex-1 truncate">
              {running ? "客户端运行中" : "客户端未运行"}
            </span>
          </TooltipTrigger>
          <TooltipContent side="top">
            {(status?.clients ?? [])
              .map((c) => `${c.clientName}：${c.running ? "运行中" : "未运行"}`)
              .join("；") || "状态未知"}
          </TooltipContent>
        </Tooltip>
        <span className="shrink-0 text-sidebar-foreground/50">v{version || "?"}</span>
      </div>
    </section>
  );
}

function Layout() {
  const hasUnifiedTitleBar =
    !isWebui() &&
    typeof navigator !== "undefined" &&
    navigator.userAgent.includes("Macintosh");

  return (
    <div className="flex h-screen min-h-0 overflow-hidden bg-background">
      {hasUnifiedTitleBar ? (
        <div
          data-tauri-drag-region
          className="fixed inset-x-0 top-0 z-50 h-8"
          aria-hidden="true"
        />
      ) : null}
      <aside
        className={cn(
          "flex min-h-0 w-[220px] shrink-0 flex-col border-r border-sidebar-border bg-sidebar px-3 pb-4",
          hasUnifiedTitleBar ? "pt-20" : "pt-4",
        )}
      >
        <div className="flex items-center gap-2.5 px-1 pb-5">
          <AppIconMark size={36} className="drop-shadow-sm" />
          <div className="min-w-0">
            <div
              className="truncate text-[15px] leading-5 tracking-[-0.02em] text-sidebar-foreground/90"
              style={{
                fontFamily:
                  '"Bricolage Grotesque Variable", "SF Pro Display", ui-sans-serif, sans-serif',
                fontWeight: 640,
              }}
            >
              TT Switch
            </div>
          </div>
        </div>
        <nav className="flex min-h-0 flex-1 flex-col gap-0.5" aria-label="主导航">
          {NAV_ITEMS.map(({ to, label, icon: Icon, ...rest }) => (
            <NavLink key={to} to={to} className={navClass} {...("end" in rest ? { end: true } : {})}>
              <Icon className="size-4" />
              {label}
            </NavLink>
          ))}
        </nav>
        <SidebarFooter />
      </aside>
      <main
        className={cn(
          "min-w-0 flex-1 overflow-y-auto overscroll-contain bg-background",
          hasUnifiedTitleBar && "pt-16 [&>div]:pt-4",
        )}
      >
        <Outlet />
      </main>
    </div>
  );
}

export default function App() {
  return (
    <TooltipProvider delayDuration={250}>
      <HashRouter>
        <Routes>
          {/* Companion 悬浮窗：独立无侧栏布局 */}
          <Route path="/companion" element={<CompanionPage />} />
          <Route element={<Layout />}>
            <Route path="/" element={<AccountsPage />} />
            <Route path="/sessions" element={<SessionsPage />} />
            <Route path="/token-stats" element={<TokenStatsPage />} />
            <Route path="/credit-stats" element={<CreditStatsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
        <Toaster />
      </HashRouter>
    </TooltipProvider>
  );
}
