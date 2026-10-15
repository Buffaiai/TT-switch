import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  Download,
  FileDown,
  FileUp,
  Loader2,
  Plus,
  QrCode,
  RefreshCw,
} from "lucide-react";

import { AccountCard } from "@/components/account-card";
import { PermissionBanner, ProgressPanel } from "@/components/operation-progress";
import { TraeIdeMark, TraeSoloMark } from "@/components/product-marks";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { api, desktopApi, isWebui, type CheckinSummary } from "@/lib/api";
import { describeCheckin } from "@/lib/checkin";
import { isClientVisible, usePrefsSync } from "@/lib/prefs";
import { useVisibleInterval } from "@/lib/use-visible-interval";
import type { AccountMeta, ClientKind, TransferPreview } from "@/lib/types";
import { cn } from "@/lib/utils";
import { useAccountsStore } from "@/stores/accounts";

const CLIENT_ORDER: ClientKind[] = ["ide_cn", "solo_cn", "ide_intl"];
const desktop = !isWebui();

/** 档位标识的悬浮说明（界面可见文案统一为「Trae」，仅以图标与提示区分）。 */
const CLIENT_VARIANT_LABEL: Record<ClientKind, string> = {
  ide_cn: "Trae（IDE 国内版）",
  solo_cn: "Trae（SOLO 国内版）",
  ide_intl: "Trae（IDE 国际版）",
};

function clientMark(client: ClientKind) {
  const label = CLIENT_VARIANT_LABEL[client] ?? "Trae";
  return client === "solo_cn" ? (
    <TraeSoloMark label={label} />
  ) : (
    <TraeIdeMark label={label} />
  );
}

/** 归并卡片上的批量操作目标：同一身份（uid）下的多份档位快照。 */
type GroupTarget = {
  key: string;
  name: string;
  records: AccountMeta[];
};

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

export default function AccountsPage() {
  const {
    status,
    accounts,
    systemInfo,
    checks,
    progress,
    refresh,
    refreshProgress,
    upsertCheck,
  } = useAccountsStore();
  const [busy, setBusy] = useState<string | null>(null);
  const [switchTarget, setSwitchTarget] = useState<GroupTarget | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<GroupTarget | null>(null);
  const [deleteSelected, setDeleteSelected] = useState<Set<string>>(new Set());
  const [importPreview, setImportPreview] = useState<TransferPreview | null>(null);
  const [importSelected, setImportSelected] = useState<Set<number>>(new Set());
  const [importing, setImporting] = useState(false);
  const [, setPrefsVersion] = useState(0);
  const [checkin, setCheckin] = useState<CheckinSummary | null>(null);
  const pollRef = useRef<number | null>(null);
  const navigate = useNavigate();

  // 签到状态：页面级拉取一次聚合成 map 注入卡片；可见时每 60s 刷新。
  useVisibleInterval(
    useCallback(() => {
      api.getCheckinStatus().then(setCheckin).catch(() => undefined);
    }, []),
    60_000,
  );

  // 偏好变化（客户端可见性开关）强制重渲染。
  usePrefsSync(
    useCallback(() => setPrefsVersion((v) => v + 1), []),
  );

  useEffect(() => {
    refresh().catch((e) =>
      toast.error("加载状态失败", {
        description: e instanceof Error ? e.message : String(e),
      }),
    );
  }, [refresh]);

  // 已安装客户端静默权限自检，结果体现在横幅。
  useEffect(() => {
    if (!status) return;
    for (const c of status.clients.filter((x) => x.installed)) {
      api
        .checkPermission(c.client)
        .then(upsertCheck)
        .catch(() => undefined);
    }
  }, [status, upsertCheck]);

  // 切换进度轮询：结束后自动刷新列表。
  useEffect(() => {
    const poll = async () => {
      const p = await refreshProgress();
      if (p && !p.running && pollRef.current !== null) {
        window.clearInterval(pollRef.current);
        pollRef.current = null;
        void refresh();
      }
    };
    pollRef.current = window.setInterval(poll, 800);
    void poll();
    return () => {
      if (pollRef.current !== null) window.clearInterval(pollRef.current);
    };
  }, [refresh, refreshProgress]);

  /** 可见客户端 tab（未安装的默认隐藏，可在设置页开启）。 */
  const visibleClients = useMemo(
    () =>
      CLIENT_ORDER.filter((kind) => {
        const info = systemInfo?.clientDataDirs.find((c) => c.client === kind);
        return isClientVisible(kind, info?.exists ?? false);
      }),
    [systemInfo],
  );

  /** 已添加的全部快照：按可见客户端过滤。 */
  const mergedAccounts = useMemo(
    () => accounts.filter((a) => visibleClients.includes(a.client)),
    [accounts, visibleClients],
  );

  /**
   * 按身份归并：同一 uid 在多个档位各存一份快照时，合并为一张卡片。
   * uid 相同的记录云端是同一个账号，用户视角只应看到一张卡。
   */
  const accountGroups = useMemo(() => {
    const map = new Map<string, AccountMeta[]>();
    for (const a of mergedAccounts) {
      const key = a.uid ? `uid:${a.uid}` : `id:${a.id}`;
      const list = map.get(key);
      if (list) list.push(a);
      else map.set(key, [a]);
    }
    return [...map.entries()].map(([key, records]) => {
      const first = records[0]!;
      const expiries = records.map((r) => r.expiresAt);
      return {
        key,
        records,
        displayName: records.find((r) => r.nickname)?.nickname ?? first.displayName,
        idLabel: first.uid ? `UID · ${first.uid}` : `ID · ${first.id}`,
        capturedAt: Math.max(...records.map((r) => r.capturedAt ?? 0)),
        expiresAt: expiries.every((e) => e === null)
          ? null
          : Math.max(...records.map((r) => r.expiresAt ?? 0)),
        needsRelogin: records.every((r) => r.needsRelogin),
      };
    });
  }, [mergedAccounts]);

  const clientInfo = (kind: ClientKind) => status?.clients.find((c) => c.client === kind);

  /** 可添加「当前账号」的客户端：已安装且已登录。 */
  const addCandidates = visibleClients.filter((kind) => {
    const info = clientInfo(kind);
    return Boolean(info?.installed && info.loggedIn);
  });

  /** 该快照在其所属客户端下是否为当前登录态。 */
  const isAccountCurrent = (acc: AccountMeta) => {
    const info = clientInfo(acc.client);
    return Boolean(info?.loggedIn && info.currentUid === acc.uid);
  };

  /** 卡片上可执行切换的档位：该档位客户端已安装。 */
  const switchableRecords = (records: AccountMeta[]) =>
    records.filter((r) => Boolean(clientInfo(r.client)?.installed));

  /** 卡片顶部的档位图标（同一身份可能覆盖多个档位）。 */
  const groupMarks = (records: AccountMeta[]) => (
    <span className="flex items-center gap-1">
      {records.map((r) => (
        <Fragment key={r.id}>{clientMark(r.client)}</Fragment>
      ))}
    </span>
  );

  /** 签到状态按身份聚合：同一 uid 云端积分相同，取任一档位的记录展示一次。 */
  const groupCheckin = (records: AccountMeta[]) => {
    for (const r of records) {
      const entry = checkin?.accounts?.[r.id];
      if (entry) return describeCheckin(entry, checkin?.today ?? "");
    }
    return undefined;
  };

  const onAddCurrent = async () => {
    if (addCandidates.length === 0) return;
    setBusy("capture");
    let added = 0;
    const failed: string[] = [];
    try {
      for (const kind of addCandidates) {
        try {
          await api.captureAccount(kind);
          added += 1;
        } catch (e) {
          failed.push(e instanceof Error ? e.message : String(e));
        }
      }
      await refresh();
      if (added > 0) {
        toast.success(added === 1 ? "当前账号已添加" : `已添加 ${added} 个账号`);
      }
      if (failed.length > 0) {
        toast.error(`${failed.length} 个客户端添加失败`, {
          description: failed.join("；"),
        });
      }
    } finally {
      setBusy(null);
    }
  };

  /** 切换：对该身份下所有「本机已安装」的档位依次执行。 */
  const onSwitchConfirm = async () => {
    const target = switchTarget;
    if (!target || target.records.length === 0) return;
    setSwitchTarget(null);
    setBusy(`switch:${target.key}`);
    let switched = 0;
    let already = 0;
    const failed: string[] = [];
    try {
      for (const rec of target.records) {
        try {
          const r = await api.switchAccount(rec.id, true);
          if (r.alreadyCurrent) already += 1;
          else switched += 1;
        } catch (e) {
          failed.push(e instanceof Error ? e.message : String(e));
        }
      }
      await refresh();
      if (switched > 0) {
        toast.success(`已切换到「${target.name}」`, {
          description: `共处理 ${switched} 个客户端`,
        });
      } else if (already > 0) {
        toast.success("当前已是该账号");
      }
      if (failed.length > 0) {
        toast.error(`${failed.length} 个客户端切换失败`, { description: failed.join("；") });
      }
    } finally {
      setBusy(null);
    }
  };

  /** 删除：按弹窗中勾选的档位逐条删除快照。 */
  const onDeleteConfirm = async () => {
    const target = deleteTarget;
    if (!target) return;
    const ids = target.records.filter((r) => deleteSelected.has(r.id)).map((r) => r.id);
    if (ids.length === 0) return;
    setDeleteTarget(null);
    setBusy(`delete:${target.key}`);
    try {
      for (const id of ids) await api.deleteAccount(id);
      await refresh();
      toast.success(`账号「${target.name}」已删除`, {
        description: `共删除 ${ids.length} 份快照`,
      });
    } catch (e) {
      toast.error("删除失败", { description: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(null);
    }
  };

  /** 标注：同身份的多个档位快照一起更新，保持一个名字。 */
  const onRename = async (records: AccountMeta[], nickname: string) => {
    try {
      for (const rec of records) await api.renameAccount(rec.id, nickname);
      await refresh();
      toast.success("标注已更新");
    } catch (e) {
      toast.error("更新标注失败", { description: e instanceof Error ? e.message : String(e) });
    }
  };

  const onExportAll = async () => {
    try {
      const r = await desktopApi.exportAccounts([]);
      if (r) {
        toast.success("账号已导出", {
          description: `${r.count} 个账号（${fmtSize(r.sizeBytes)}）`,
        });
      }
    } catch (e) {
      toast.error("导出失败", { description: e instanceof Error ? e.message : String(e) });
    }
  };

  const onPickImport = async () => {
    try {
      const p = await desktopApi.previewImport();
      if (p) {
        setImportPreview(p);
        setImportSelected(new Set(p.accounts.map((_, i) => i)));
      }
    } catch (e) {
      toast.error("读取迁移包失败", { description: e instanceof Error ? e.message : String(e) });
    }
  };

  const onConfirmImport = async () => {
    if (!importPreview) return;
    const indexes = [...importSelected].sort((a, b) => a - b);
    if (indexes.length === 0) return;
    setImporting(true);
    try {
      const r = await desktopApi.importAccounts(importPreview.path, indexes);
      const parts = [`成功导入 ${r.imported.length} 个账号`];
      if (r.skipped.length > 0) parts.push(`跳过 ${r.skipped.length} 个`);
      toast.success(r.imported.length > 0 ? "导入完成" : "没有新账号导入", {
        description: parts.join("，"),
      });
      setImportPreview(null);
      await refresh();
    } catch (e) {
      toast.error("导入失败", { description: e instanceof Error ? e.message : String(e) });
    } finally {
      setImporting(false);
    }
  };

  return (
    <div className="mx-auto w-full min-w-0 max-w-5xl px-4 py-6 sm:px-6 sm:py-8">
      <header className="mb-6 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">账号管理</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            采集 Trae 当前登录态，随时一键切换。
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="gap-1.5"
            onClick={() => toast.info("扫码登录正在接入 Trae 账号体系，下一阶段提供")}
          >
            <QrCode className="size-4" />
            扫码添加账号
          </Button>
          {desktop && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="outline" size="sm" className="gap-1.5">
                  <Download className="size-4" />
                  迁移
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onSelect={() => void onExportAll()}>
                  <FileUp />
                  导出全部账号…
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => void onPickImport()}>
                  <FileDown />
                  导入账号…
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="gap-1.5"
            disabled={busy !== null}
            onClick={() => void refresh()}
          >
            <RefreshCw className={cn("size-4", busy === "refresh" && "animate-spin")} />
            刷新
          </Button>
        </div>
      </header>

      <PermissionBanner
        checks={checks}
        onOpenSettings={() => {
          void api
            .openPrivacySettings("all_files")
            .catch((e) =>
              toast.error("打开系统设置失败", {
                description: e instanceof Error ? e.message : String(e),
              }),
            );
        }}
        onRevealApp={() => void api.reveal("clientApp", "ide_cn").catch(() => undefined)}
      />

      <ProgressPanel progress={progress} />

      {visibleClients.length === 0 ? (
        <p className="rounded-xl border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
          没有可见的客户端。请在设置页开启至少一个客户端的显示。
        </p>
      ) : (
        <section className="mt-5">
          <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-baseline gap-2">
              <h2 className="text-base font-medium">Trae</h2>
              <span className="text-sm text-muted-foreground">
                已添加 {accountGroups.length} 个账号
              </span>
            </div>
            <Button
              type="button"
              size="sm"
              className="gap-1.5"
              disabled={
                busy !== null || progress?.running === true || addCandidates.length === 0
              }
              onClick={() => void onAddCurrent()}
            >
              {busy === "capture" ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Plus className="size-4" />
              )}
              {busy === "capture" ? "采集中…" : "添加当前账号"}
            </Button>
          </div>

          {accountGroups.length === 0 ? (
            <p className="rounded-xl border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
              暂无账号。在 Trae 中登录后点击「添加当前账号」。
            </p>
          ) : (
            <div className="grid gap-3 sm:grid-cols-2">
              {accountGroups.map((g) => (
                <AccountCard
                  key={g.key}
                  displayName={g.displayName}
                  idLabel={g.idLabel}
                  clientMarks={groupMarks(g.records)}
                  capturedAt={g.capturedAt}
                  expiresAt={g.expiresAt}
                  needsRelogin={g.needsRelogin}
                  isCurrent={g.records.some(isAccountCurrent)}
                  switching={busy === `switch:${g.key}`}
                  busy={busy !== null || progress?.running === true}
                  switchable={switchableRecords(g.records).length > 0}
                  checkin={groupCheckin(g.records)}
                  onOpenCreditStats={() => navigate("/credit-stats")}
                  onSwitch={() =>
                    setSwitchTarget({
                      key: g.key,
                      name: g.displayName,
                      records: switchableRecords(g.records),
                    })
                  }
                  onDelete={() => {
                    setDeleteSelected(new Set(g.records.map((r) => r.id)));
                    setDeleteTarget({ key: g.key, name: g.displayName, records: g.records });
                  }}
                  onRename={(nickname) => void onRename(g.records, nickname)}
                />
              ))}
            </div>
          )}
        </section>
      )}

      {/* 切换确认：同一身份下所有已安装档位一起切换 */}
      <AlertDialog open={switchTarget !== null} onOpenChange={(open) => !open && setSwitchTarget(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>切换到「{switchTarget?.name}」？</AlertDialogTitle>
            <AlertDialogDescription>
              将关闭并重启该账号覆盖的 {switchTarget?.records.length ?? 0} 个已安装客户端，当前登录态会自动备份。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction onClick={() => void onSwitchConfirm()}>切换</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* 删除确认：按档位勾选要删除的快照 */}
      <AlertDialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除账号「{deleteTarget?.name}」的快照？</AlertDialogTitle>
            <AlertDialogDescription>
              勾选要删除的档位快照，此操作不可撤销。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <ul className="space-y-2 text-sm">
            {deleteTarget?.records.map((r) => (
              <li key={r.id} className="flex items-center gap-2">
                <input
                  type="checkbox"
                  id={`delete-${r.id}`}
                  className="size-3.5"
                  checked={deleteSelected.has(r.id)}
                  onChange={() => {
                    const next = new Set(deleteSelected);
                    if (next.has(r.id)) next.delete(r.id);
                    else next.add(r.id);
                    setDeleteSelected(next);
                  }}
                />
                <label htmlFor={`delete-${r.id}`} className="flex items-center gap-1.5">
                  {clientMark(r.client)}
                  <span>{CLIENT_VARIANT_LABEL[r.client]}</span>
                </label>
              </li>
            ))}
          </ul>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              disabled={deleteSelected.size === 0}
              onClick={() => void onDeleteConfirm()}
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* 导入预览 */}
      <Dialog open={importPreview !== null} onOpenChange={(open) => !open && setImportPreview(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>选择要导入的账号（共 {importPreview?.accounts.length ?? 0} 个）</DialogTitle>
            <DialogDescription>
              与本机同客户端、同 uid 的账号会自动跳过。
            </DialogDescription>
          </DialogHeader>
          <ul className="max-h-64 space-y-1 overflow-y-auto text-sm">
            {importPreview?.accounts.map((a, i) => (
              <li key={`${a.id}-${i}`} className="flex items-center gap-2">
                <input
                  type="checkbox"
                  className="size-3.5"
                  checked={importSelected.has(i)}
                  onChange={() => {
                    const next = new Set(importSelected);
                    if (next.has(i)) next.delete(i);
                    else next.add(i);
                    setImportSelected(next);
                  }}
                />
                <span>
                  {a.clientName} · {a.displayName} · {a.snapshotFiles} 个文件
                </span>
                {a.conflict === "sameAccount" && (
                  <span className="rounded bg-amber-100 px-1.5 py-0.5 text-xs text-amber-700 dark:bg-amber-950 dark:text-amber-400">
                    本机已有，将跳过
                  </span>
                )}
                {a.conflict === "sameId" && (
                  <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                    id 冲突，将重新分配
                  </span>
                )}
              </li>
            ))}
          </ul>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setImportPreview(null)}>
              取消
            </Button>
            <Button
              type="button"
              disabled={importSelected.size === 0 || importing}
              onClick={() => void onConfirmImport()}
            >
              {importing ? "导入中…" : `确认导入（${importSelected.size}）`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
