import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import type { PermissionCheck, SwitchProgress } from "@/lib/types";

/** 切换/操作进度面板：展示后端进度消息流，失败时红色高亮最后一条原因。 */
export function ProgressPanel({ progress }: { progress: SwitchProgress | null }) {
  const messages = progress?.messages ?? [];
  if (messages.length === 0) return null;
  const running = progress?.running === true;
  const last = messages[messages.length - 1]?.message ?? "";
  const failed = !running && last.startsWith("切换失败");
  return (
    <Alert variant={failed ? "destructive" : "default"} className="mb-4">
      <AlertTitle className="flex items-center gap-2">
        {running && (
          <span className="inline-block size-3 animate-spin rounded-full border-2 border-current border-t-transparent" />
        )}
        {running ? "操作进行中" : failed ? "操作失败" : "操作完成"}
      </AlertTitle>
      <AlertDescription>
        <ul className="space-y-0.5">
          {messages.map((m, i) => (
            <li key={i} className="font-mono text-xs opacity-90">
              {m.message}
            </li>
          ))}
        </ul>
      </AlertDescription>
    </Alert>
  );
}

/** 数据目录写入受限横幅：引导用户前往系统设置授予权限。 */
export function PermissionBanner({
  checks,
  onOpenSettings,
  onRevealApp,
}: {
  checks: PermissionCheck[];
  onOpenSettings: () => void;
  onRevealApp: () => void;
}) {
  const denied = checks.filter((c) => !c.ok);
  if (denied.length === 0) return null;
  const first = denied[0]!;
  return (
    <Alert className="mb-4 border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-200">
      <AlertTitle>数据目录写入受限（{denied.length} 个客户端）</AlertTitle>
      <AlertDescription>
        <p className="mt-1 text-xs">{first.hint}</p>
        {first.error && <p className="mt-1 font-mono text-xs opacity-75">{first.error}</p>}
        <div className="mt-2 flex gap-2">
          <button
            type="button"
            className="rounded-md bg-amber-600 px-2.5 py-1 text-xs text-white hover:bg-amber-500"
            onClick={onOpenSettings}
          >
            打开系统设置
          </button>
          <button
            type="button"
            className="rounded-md border border-amber-400 px-2.5 py-1 text-xs hover:bg-amber-100 dark:hover:bg-amber-900"
            onClick={onRevealApp}
          >
            在 Finder 中显示 App
          </button>
        </div>
      </AlertDescription>
    </Alert>
  );
}
