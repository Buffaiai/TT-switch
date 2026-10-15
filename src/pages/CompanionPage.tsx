import { useCallback, useEffect, useRef, useState } from "react";
import { Navigate } from "react-router-dom";
import { toast } from "sonner";
import { RefreshCw, X } from "lucide-react";

import { AppIconMark, StatusDot } from "@/components/product-marks";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api, desktopApi, isWebui } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useAccountsStore } from "@/stores/accounts";

const CLIENT_LABELS: Record<string, string> = {
  ide_cn: "Trae",
  solo_cn: "Trae",
  ide_intl: "Trae",
};

/** Companion 悬浮窗：置顶紧凑面板，当前状态 + 快捷切换。 */
export default function CompanionPage() {
  const { status, accounts, refresh, refreshProgress, progress } = useAccountsStore();
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const pollRef = useRef<number | null>(null);

  const doRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await refresh();
    } catch {
      /* 悬浮窗静默重试 */
    } finally {
      setRefreshing(false);
    }
  }, [refresh]);

  // 首次加载 + 每 30s 自动刷新；卸载时停止切换轮询。
  useEffect(() => {
    void doRefresh();
    const timer = window.setInterval(doRefresh, 30_000);
    return () => {
      window.clearInterval(timer);
      if (pollRef.current !== null) window.clearInterval(pollRef.current);
    };
  }, [doRefresh]);

  if (isWebui()) return <Navigate to="/" replace />;

  const switchTo = async (id: string, name: string) => {
    if (switchingId) return;
    setSwitchingId(id);
    try {
      await api.switchAccount(id, true);
      toast.info(`正在切换到 ${name}…`);
      pollRef.current = window.setInterval(async () => {
        const p = await refreshProgress();
        if (p && !p.running) {
          if (pollRef.current !== null) window.clearInterval(pollRef.current);
          pollRef.current = null;
          setSwitchingId(null);
          toast.success(`${name} 切换完成`);
          void doRefresh();
        }
      }, 800);
    } catch (e) {
      setSwitchingId(null);
      toast.error("切换失败", {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  };

  const closeWindow = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    getCurrentWindow().close();
  };

  return (
    <div className="flex h-screen min-h-0 flex-col overflow-hidden rounded-lg bg-background text-foreground">
      {/* 拖拽区标题栏 */}
      <header
        data-tauri-drag-region
        className="flex shrink-0 items-center gap-2 border-b border-sidebar-border bg-sidebar px-3 py-2.5"
      >
        <AppIconMark size={22} />
        <span className="flex-1 text-[13px] font-medium tracking-tight">TT Switch</span>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          onClick={doRefresh}
          aria-label="刷新"
        >
          <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          onClick={closeWindow}
          aria-label="关闭悬浮窗"
        >
          <X className="size-3.5" />
        </Button>
      </header>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
        {/* 客户端状态 */}
        <section>
          <div className="mb-1.5 text-xs text-muted-foreground">客户端</div>
          <div className="space-y-1">
            {(status?.clients ?? []).map((c) => (
              <div
                key={c.client}
                className="flex items-center gap-2 rounded-md px-2 py-1.5 text-[13px]"
              >
                <StatusDot on={c.running} />
                <span className="min-w-0 flex-1 truncate">{c.clientName}</span>
                {c.currentUid ? (
                  <span className="shrink-0 text-xs text-muted-foreground">
                    …{c.currentUid.slice(-4)}
                  </span>
                ) : (
                  <span className="shrink-0 text-xs text-muted-foreground">未登录</span>
                )}
              </div>
            ))}
            {!status && (
              <div className="px-2 py-1.5 text-[13px] text-muted-foreground">状态读取中…</div>
            )}
          </div>
        </section>

        {/* 账号快捷切换 */}
        <section>
          <div className="mb-1.5 text-xs text-muted-foreground">账号</div>
          <div className="space-y-1">
            {accounts.map((a) => {
              const busy = switchingId === a.id;
              return (
                <div
                  key={a.id}
                  className="flex items-center gap-2 rounded-md border px-2 py-1.5"
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] font-medium">{a.displayName}</div>
                    <div className="text-[11px] text-muted-foreground">
                      {CLIENT_LABELS[a.client] ?? a.client}
                      {a.needsRelogin ? " · 需重新登录" : ""}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-6 px-2 text-xs"
                    disabled={switchingId !== null || Boolean(progress?.running)}
                    onClick={() => switchTo(a.id, a.displayName)}
                  >
                    {busy ? "切换中…" : "切换"}
                  </Button>
                </div>
              );
            })}
            {accounts.length === 0 && (
              <div className="px-2 py-2 text-[13px] text-muted-foreground">
                暂无账号，请在主窗口添加
              </div>
            )}
          </div>
        </section>
      </div>

      <footer className="shrink-0 border-t border-sidebar-border px-3 py-2">
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-full justify-center text-xs text-muted-foreground"
          onClick={() => {
            void desktopApi.showMainWindow();
          }}
        >
          <Badge variant="outline" className="mr-1.5 text-[10px]">
            主窗口
          </Badge>
          打开完整控制台
        </Button>
      </footer>
    </div>
  );
}
