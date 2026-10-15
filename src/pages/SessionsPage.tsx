import { Link2 } from "lucide-react";

/**
 * 关联会话：跨账号复制/关联会话记录。
 *
 * 后端接入中：Trae 的会话存储解析与会话组同步将在后续阶段提供，
 * 页面结构已按 workbuddy-switch 复刻预留。
 */
export default function SessionsPage() {
  return (
    <div className="mx-auto w-full min-w-0 max-w-5xl px-4 py-6 sm:px-6 sm:py-8">
      <header className="mb-6">
        <h1 className="text-2xl font-semibold tracking-tight">关联会话</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          在账号之间复制会话、建立跨账号关联，保持多账号会话连续。
        </p>
      </header>
      <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed px-6 py-16 text-center">
        <Link2 className="size-8 text-muted-foreground/50" />
        <div className="text-sm font-medium">会话关联能力接入中</div>
        <p className="max-w-md text-xs leading-5 text-muted-foreground">
          正在适配 Trae 的会话存储格式
          （会话列表、跨账号复制、关联组同步）。
          完成后此页面将提供与参考项目一致的功能。
        </p>
      </div>
    </div>
  );
}
