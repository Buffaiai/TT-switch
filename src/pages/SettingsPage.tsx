import { useCallback, useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { TimePicker } from "@/components/ui/time-picker";
import { api, desktopApi, isWebui } from "@/lib/api";
import {
  getCompactMode,
  isClientVisible,
  setClientVisible,
  setCompactMode,
  usePrefsSync,
} from "@/lib/prefs";
import {
  getThemePreference,
  setThemePreference,
  type ThemePreference,
} from "@/lib/theme";
import type { BackupInfo, ClientKind, ErrorLogEntry } from "@/lib/types";
import { useAccountsStore } from "@/stores/accounts";

const desktop = !isWebui();

function fmtTime(ms: number | null | undefined): string {
  if (!ms) return "-";
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

/** 设置区块：标题 + 说明 + 卡片行。 */
function Section({
  title,
  description,
  children,
  action,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <section className="mb-6">
      <div className="mb-2 flex items-center justify-between">
        <h2 className="text-[15px] font-medium">{title}</h2>
        {action}
      </div>
      {description && (
        <div className="mb-2 text-xs leading-5 text-muted-foreground">{description}</div>
      )}
      <div className="divide-y divide-border rounded-xl border bg-card">{children}</div>
    </section>
  );
}

/** 设置行：左标签 + 说明，右侧控件。 */
function Row({
  label,
  description,
  children,
}: {
  label: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3">
      <div className="min-w-0">
        <div className="text-sm">{label}</div>
        {description && (
          <div className="mt-0.5 text-xs text-muted-foreground">{description}</div>
        )}
      </div>
      {children && <div className="shrink-0">{children}</div>}
    </div>
  );
}

export default function SettingsPage() {
  const { status, systemInfo, checks, refresh, upsertCheck } = useAccountsStore();
  const [theme, setTheme] = useState<ThemePreference>(getThemePreference);
  const [compact, setCompact] = useState(getCompactMode);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [checkin, setCheckin] = useState<{ enabled: boolean; time: string } | null>(null);
  const [logs, setLogs] = useState<{ path: string; logs: ErrorLogEntry[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const [, setPrefsVersion] = useState(0);

  usePrefsSync(
    useCallback(() => {
      setCompact(getCompactMode());
      setPrefsVersion((v) => v + 1);
    }, []),
  );

  useEffect(() => {
    if (desktop) {
      desktopApi
        .getLaunchAtLogin()
        .then(setAutostart)
        .catch(() => setAutostart(null));
    }
    api
      .getCheckinStatus()
      .then((s) => setCheckin({ enabled: s.enabled, time: s.time }))
      .catch(() => setCheckin(null));
  }, []);

  const updateCheckin = async (patch: { enabled?: boolean; time?: string }) => {
    const prev = checkin;
    setCheckin((c) => (c ? { ...c, ...patch } : c));
    try {
      const s = await api.setCheckinConfig(patch.enabled, patch.time);
      setCheckin({ enabled: s.enabled, time: s.time });
    } catch (e) {
      setCheckin(prev);
      toast.error("保存签到设置失败", {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  };

  const reveal = (target: string, client?: ClientKind) => {
    void api
      .reveal(target, client)
      .catch((e) =>
        toast.error("打开失败", { description: e instanceof Error ? e.message : String(e) }),
      );
  };

  const runPermissionCheck = async (client: ClientKind) => {
    try {
      const r = await api.checkPermission(client);
      upsertCheck(r);
      toast[r.ok ? "success" : "error"](
        `${r.client === client ? "" : ""}${r.ok ? "可写" : "受限"}`,
        { description: r.ok ? undefined : r.message },
      );
    } catch (e) {
      toast.error("权限自检失败", { description: e instanceof Error ? e.message : String(e) });
    }
  };

  const onDeleteBackup = async (b: BackupInfo) => {
    if (!window.confirm(`删除备份 ${b.name}？`)) return;
    setBusy(true);
    try {
      await api.deleteBackup(b.name);
      await refresh();
      toast.success("备份已删除");
    } catch (e) {
      toast.error("删除失败", { description: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  const onLoadLogs = async () => {
    try {
      setLogs(await api.getErrorLogs());
    } catch (e) {
      toast.error("读取日志失败", { description: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <div className="mx-auto w-full min-w-0 max-w-3xl px-4 py-6 sm:px-6 sm:py-8">
      <h1 className="text-2xl font-semibold tracking-tight">设置</h1>
      <p className="mt-1 text-sm text-muted-foreground">
        外观、客户端显示、系统集成与数据管理。
      </p>

      {/* 外观 */}
      <div className="mt-6" />
      <Section title="外观">
        <Row label="主题" description="选择浅色、深色，或跟随系统外观自动切换">
          <Select
            value={theme}
            onValueChange={(v) => {
              const value = v as ThemePreference;
              setTheme(value);
              setThemePreference(value);
            }}
          >
            <SelectTrigger className="w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="system">跟随系统</SelectItem>
              <SelectItem value="light">浅色</SelectItem>
              <SelectItem value="dark">深色</SelectItem>
            </SelectContent>
          </Select>
        </Row>
        <Row label="紧凑账号列表" description="减小账号卡片信息密度，一屏显示更多账号">
          <Switch
            checked={compact}
            onCheckedChange={(v) => {
              setCompact(v);
              setCompactMode(v);
            }}
          />
        </Row>
      </Section>

      {/* 显示客户端 */}
      <Section
        title="显示客户端"
        description="关闭后该客户端不再出现在账号页，账号与快照不受影响。"
      >
        {(systemInfo?.clientDataDirs ?? []).map((c) => (
          <Row
            key={c.client}
            label={c.clientName}
            description={c.exists ? "已安装" : "未安装（默认隐藏）"}
          >
            <Switch
              checked={isClientVisible(c.client, c.exists)}
              onCheckedChange={(v) => setClientVisible(c.client, v)}
            />
          </Row>
        ))}
      </Section>

      {/* 每日自动签到 */}
      <Section
        title="每日自动签到"
        description="到点后自动为全部已保存账号领取签到积分；App 驻留菜单栏期间生效（窗口关闭不影响）。"
      >
        <Row label="自动签到" description="使用各账号快照凭据调用官方签到接口，同日幂等不重复领取">
          <Switch
            checked={checkin?.enabled ?? false}
            disabled={checkin === null}
            onCheckedChange={(v) => void updateCheckin({ enabled: v })}
          />
        </Row>
        <Row label="签到时间" description="每日触发时间；当日失败会自动延迟重试">
          <TimePicker
            value={checkin?.time ?? ""}
            disabled={checkin === null}
            onChange={(time) => void updateCheckin({ time })}
          />
        </Row>
      </Section>

      {/* 系统集成 */}
      {desktop && (
        <Section title="系统集成">
          <Row label="开机时静默启动" description="登录系统后驻留菜单栏，不弹出窗口">
            <Switch
              checked={autostart === true}
              disabled={autostart === null}
              onCheckedChange={async (v) => {
                setAutostart(v);
                try {
                  await desktopApi.setLaunchAtLogin(v);
                  toast.success(v ? "已开启开机自启" : "已关闭开机自启");
                } catch {
                  setAutostart(!v);
                  toast.error("设置失败，请重试");
                }
              }}
            />
          </Row>
          <Row label="Companion 悬浮窗" description="置顶小窗：当前账号状态 + 快捷切换">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              onClick={() => {
                desktopApi.openCompanion().catch((e) =>
                  toast.error("打开悬浮窗失败", {
                    description: e instanceof Error ? e.message : String(e),
                  }),
                );
              }}
            >
              打开
            </Button>
          </Row>
        </Section>
      )}

      {/* 权限自检 */}
      <Section
        title="权限自检"
        description="切换账号需要写入各客户端数据目录。macOS 若提示受限，请授予「完全磁盘访问权限」。"
        action={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 text-xs"
            onClick={() =>
              status?.clients.forEach((c) => void runPermissionCheck(c.client))
            }
          >
            全部自检
          </Button>
        }
      >
        {(systemInfo?.clientDataDirs ?? []).map((c) => {
          const result = checks.find((x) => x.client === c.client);
          return (
            <Row key={c.client} label={c.clientName}>
              <div className="flex items-center gap-2">
                {result ? (
                  <span
                    className={
                      result.ok
                        ? "text-xs text-emerald-600 dark:text-emerald-400"
                        : "text-xs text-destructive"
                    }
                  >
                    {result.ok ? "可写" : "受限"}
                  </span>
                ) : (
                  <span className="text-xs text-muted-foreground">未检测</span>
                )}
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-7 text-xs"
                  onClick={() => void runPermissionCheck(c.client)}
                >
                  检测
                </Button>
              </div>
            </Row>
          );
        })}
        <Row label="系统设置">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            onClick={() =>
              void api
                .openPrivacySettings("all_files")
                .catch((e) =>
                  toast.error("打开失败", {
                    description: e instanceof Error ? e.message : String(e),
                  }),
                )
            }
          >
            打开「隐私与安全性」
          </Button>
        </Row>
      </Section>

      {/* 存储位置 */}
      <Section title="存储位置">
        <Row label="数据与账号快照" description={systemInfo?.storeDir ?? "~/.tt-switch"}>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            onClick={() => reveal("store")}
          >
            在 Finder 中显示
          </Button>
        </Row>
        <Row label="切换前安全备份" description={systemInfo?.backupsDir ?? "~/.tt-switch/backups"}>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            onClick={() => reveal("backups")}
          >
            在 Finder 中显示
          </Button>
        </Row>
        {(systemInfo?.clientDataDirs ?? []).map((c) => (
          <Row key={c.client} label={`${c.clientName} 数据目录`} description={c.dir}>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              disabled={!c.exists}
              onClick={() => reveal("clientData", c.client)}
            >
              在 Finder 中显示
            </Button>
          </Row>
        ))}
      </Section>

      {/* 安全备份 */}
      <Section
        title="安全备份"
        description="每端保留最近 10 份，切换账号时自动创建。"
        action={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-xs"
            disabled={busy}
            onClick={() => void refresh()}
          >
            <RefreshCw className="size-3.5" />
            刷新
          </Button>
        }
      >
        {!systemInfo || systemInfo.backups.length === 0 ? (
          <Row
            label={<span className="text-muted-foreground">暂无备份</span>}
          />
        ) : (
          systemInfo.backups.map((b) => (
            <Row
              key={b.name}
              label={
                <span className="font-mono text-xs">
                  {b.name} · {fmtSize(b.sizeBytes)} · {fmtTime(b.atMs)}
                </span>
              }
            >
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="h-7 text-xs text-destructive hover:bg-destructive/10 hover:text-destructive"
                disabled={busy}
                onClick={() => void onDeleteBackup(b)}
              >
                删除
              </Button>
            </Row>
          ))
        )}
      </Section>

      {/* 错误日志 */}
      <Section
        title="错误日志"
        description="最近 200 条本机错误记录，仅保存在本地。"
        action={
          <div className="flex gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={() => void onLoadLogs()}
            >
              {logs ? "刷新" : "查看"}
            </Button>
            {logs && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 text-xs"
                onClick={() => reveal("errorLog")}
              >
                在 Finder 中显示
              </Button>
            )}
          </div>
        }
      >
        {logs === null ? (
          <Row label={<span className="text-muted-foreground">点击「查看」加载日志</span>} />
        ) : logs.logs.length === 0 ? (
          <Row label={<span className="text-muted-foreground">暂无错误记录</span>} />
        ) : (
          <div className="max-h-56 overflow-y-auto px-4 py-2">
            <ul className="space-y-1">
              {logs.logs.slice(0, 50).map((l, i) => (
                <li key={i} className="font-mono text-xs text-muted-foreground">
                  <span className="opacity-60">{fmtTime(l.at)}</span>
                  <span className="mx-1 rounded bg-muted px-1">{l.kind}</span>
                  {l.message}
                </li>
              ))}
            </ul>
          </div>
        )}
      </Section>

      <p className="mt-10 text-xs text-muted-foreground">
        数据目录 ~/.tt-switch · 仅本地运行，凭据不上传 · 关闭窗口后驻留菜单栏
      </p>
    </div>
  );
}
