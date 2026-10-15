import { useState, type ReactNode } from "react";
import { ArrowRight, Loader2, Sparkles, Trash2 } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { CheckinDisplay } from "@/lib/checkin";

const AVATAR_TONES = [
  "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300",
  "bg-violet-100 text-violet-800 dark:bg-violet-950 dark:text-violet-300",
  "bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300",
  "bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300",
  "bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300",
  "bg-teal-100 text-teal-800 dark:bg-teal-950 dark:text-teal-300",
] as const;

function avatarTone(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return AVATAR_TONES[hash % AVATAR_TONES.length];
}

function avatarInitial(name: string): string {
  const trimmed = name.trim();
  return trimmed ? trimmed[0]!.toUpperCase() : "?";
}

function fmtTime(ms: number | null | undefined): string {
  if (!ms) return "-";
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * 账号卡片：一个身份一张卡。
 * 同一 Trae 身份在多个客户端档位（IDE / SOLO）各存一份快照时，合并为一张卡展示。
 */
export function AccountCard({
  displayName,
  idLabel,
  clientMarks,
  capturedAt,
  expiresAt,
  needsRelogin,
  isCurrent,
  switching,
  busy,
  switchable,
  checkin,
  onOpenCreditStats,
  onSwitch,
  onDelete,
  onRename,
}: {
  /** 账号标注（同身份的多份快照共用一个名字） */
  displayName: string;
  /** UID / ID 展示文案 */
  idLabel: string;
  /** 该身份覆盖的客户端档位图标（可能多个） */
  clientMarks?: ReactNode;
  /** 最近一次采集时间 */
  capturedAt: number;
  /** 凭据有效期（取各档位中最晚的） */
  expiresAt: number | null;
  /** 所有档位快照均已失效 */
  needsRelogin: boolean;
  isCurrent: boolean;
  /** 该卡片正在切换中 */
  switching: boolean;
  /** 全局忙碌（禁用操作） */
  busy: boolean;
  /** 存在本机已安装的档位，可执行切换 */
  switchable: boolean;
  /** 签到状态徽章（页面级聚合后注入） */
  checkin?: CheckinDisplay;
  /** 点击签到徽章跳转积分统计页 */
  onOpenCreditStats?: () => void;
  onSwitch: () => void;
  onDelete: () => void;
  onRename: (nickname: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState("");

  const saveRename = () => {
    setEditing(false);
    if (editValue.trim() !== displayName) {
      onRename(editValue.trim());
    }
  };

  return (
    <div
      className={cn(
        "group rounded-xl border bg-card p-4 shadow-xs transition-colors",
        isCurrent ? "border-brand/40" : "border-border",
      )}
    >
      <div className="flex items-start gap-3">
        <span
          className={cn(
            "flex size-10 shrink-0 items-center justify-center rounded-full text-sm font-semibold",
            avatarTone(displayName),
          )}
          aria-hidden="true"
        >
          {avatarInitial(displayName)}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            {editing ? (
              <input
                autoFocus
                className="w-40 rounded-md border border-input bg-transparent px-2 py-0.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                value={editValue}
                onChange={(e) => setEditValue(e.target.value)}
                onBlur={saveRename}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveRename();
                  if (e.key === "Escape") setEditing(false);
                }}
              />
            ) : (
              <button
                type="button"
                className="truncate text-[15px] font-medium hover:underline"
                title="点击编辑标注"
                onClick={() => {
                  setEditing(true);
                  setEditValue(displayName);
                }}
              >
                {displayName}
              </button>
            )}
            {isCurrent && (
              <Badge className="border-0 bg-emerald-100 px-1.5 text-[10px] text-emerald-700 shadow-none dark:bg-emerald-950 dark:text-emerald-300">
                当前
              </Badge>
            )}
            {needsRelogin && (
              <Badge className="border-0 bg-amber-100 px-1.5 text-[10px] text-amber-700 shadow-none dark:bg-amber-950 dark:text-amber-300">
                需重新登录
              </Badge>
            )}
          </div>
          <div className="mt-0.5 flex items-center gap-1.5">
            {clientMarks}
            <span className="truncate font-mono text-xs text-muted-foreground">{idLabel}</span>
          </div>
          <div className="mt-1.5 space-y-0.5 text-xs text-muted-foreground">
            <div>采集于 {fmtTime(capturedAt)}</div>
            <div>
              凭据有效期至{" "}
              <span className={needsRelogin ? "text-amber-600 dark:text-amber-400" : undefined}>
                {fmtTime(expiresAt)}
              </span>
            </div>
          </div>
          {checkin && (
            <button
              type="button"
              onClick={onOpenCreditStats}
              className="mt-1.5 flex items-center gap-1.5 text-xs outline-none hover:underline"
              title="查看积分统计"
            >
              <span
                className={cn(
                  "inline-block size-1.5 rounded-full",
                  checkin.tone === "success" && "bg-emerald-500",
                  checkin.tone === "warn" && "bg-amber-500",
                  checkin.tone === "error" && "bg-red-500",
                  checkin.tone === "muted" && "bg-zinc-400/60",
                )}
              />
              <Sparkles className="size-3 text-muted-foreground" />
              <span className="text-muted-foreground">
                {checkin.credits !== null ? `积分 ${checkin.credits}` : "积分 —"}
              </span>
              <span
                className={cn(
                  checkin.tone === "success" && "text-emerald-600 dark:text-emerald-400",
                  checkin.tone === "warn" && "text-amber-600 dark:text-amber-400",
                  checkin.tone === "error" && "text-red-600 dark:text-red-400",
                  checkin.tone === "muted" && "text-muted-foreground",
                )}
              >
                · {checkin.text}
              </span>
            </button>
          )}
        </div>
      </div>
      <div className="mt-3 flex items-center justify-end gap-2 border-t border-border/60 pt-3">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 gap-1 text-xs"
          disabled={busy || isCurrent || !switchable}
          title={!switchable ? "对应的客户端未安装" : undefined}
          onClick={onSwitch}
        >
          {switching ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <ArrowRight className="size-3.5" />
          )}
          {switching
            ? "切换中…"
            : isCurrent
              ? "当前账号"
              : switchable
                ? "切换"
                : "未安装"}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 gap-1 text-xs text-destructive hover:bg-destructive/10 hover:text-destructive"
          disabled={busy}
          onClick={onDelete}
        >
          <Trash2 className="size-3.5" />
          删除
        </Button>
      </div>
    </div>
  );
}
