import { Code2, Bot } from "lucide-react";

import { cn } from "@/lib/utils";
import appIcon from "@/assets/icon.png";

/** 应用图标（侧边栏品牌区）。 */
export function AppIconMark({
  size = 36,
  className,
}: {
  size?: number;
  className?: string;
}) {
  return (
    <img
      src={appIcon}
      alt="TT Switch"
      width={size}
      height={size}
      className={cn("shrink-0 rounded-lg", className)}
      style={{ width: size, height: size }}
    />
  );
}

/** 运行状态圆点（侧边栏底部客户端状态）。 */
export function StatusDot({ on, className }: { on: boolean; className?: string }) {
  return (
    <span
      className={cn(
        "inline-block size-2 shrink-0 rounded-full",
        on ? "bg-emerald-500" : "bg-zinc-400/60",
        className,
      )}
      aria-hidden="true"
    />
  );
}

function ProductMark({
  className,
  children,
  label,
}: {
  className?: string;
  children: React.ReactNode;
  label: string;
}) {
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex size-5 shrink-0 items-center justify-center rounded-md text-white",
        className,
      )}
    >
      {children}
    </span>
  );
}

/** Trae IDE 档位标识。 */
export function TraeIdeMark({
  className,
  label = "Trae",
}: {
  className?: string;
  label?: string;
}) {
  return (
    <ProductMark label={label} className={cn("bg-sky-600", className)}>
      <Code2 className="size-3.5" strokeWidth={2.5} />
    </ProductMark>
  );
}

/** Trae SOLO 档位标识。 */
export function TraeSoloMark({
  className,
  label = "Trae",
}: {
  className?: string;
  label?: string;
}) {
  return (
    <ProductMark label={label} className={cn("bg-violet-600", className)}>
      <Bot className="size-3.5" strokeWidth={2.5} />
    </ProductMark>
  );
}
