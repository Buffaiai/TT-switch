import { useEffect, useMemo, useState } from "react";
import { MessagesSquare, RefreshCw, AlertCircle, CheckCircle2 } from "lucide-react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";

import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useAccountsStore } from "@/stores/accounts";
import { api } from "@/lib/api";

function fmtTs(ms?: number): string {
  if (!ms) return "—";
  try {
    return new Date(ms).toLocaleString("zh-CN");
  } catch {
    return String(ms);
  }
}

export default function TokenStatsPage() {
  const { accounts } = useAccountsStore();
  const [accountId, setAccountId] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [data, setData] = useState<any>(null);
  const [overview, setOverview] = useState<any>(null);

  useEffect(() => {
    if (accounts.length && !accountId) {
      setAccountId(accounts[0].id);
    }
  }, [accounts, accountId]);

  const fetch = async (id?: string) => {
    setLoading(true);
    try {
      const [res, ov] = await Promise.all([
        api.getCommercialStats(id || undefined),
        api.getCommercialLocalOverview().catch(() => null),
      ]);
      setData(res);
      setOverview(ov);
    } catch {
      setData(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (accountId) fetch(accountId);
  }, [accountId]);

  const sessionStats = data?.sessionStats ?? null;
  const sessionUsage = data?.sessionUsage ?? null;
  const usageOk = sessionUsage?.ok;
  const usageErr = sessionUsage?.error;

  const sessionUsageData = useMemo(() => {
    if (!usageOk) return null;
    const groups = sessionUsage?.data?.user_usage_group_by_session;
    if (!groups || typeof groups !== "object") return null;
    const entries = Object.entries(groups as Record<string, any>);
    let inputTokens = 0;
    let outputTokens = 0;
    let cacheTokens = 0;
    let sessions = 0;
    for (const [, v] of entries) {
      sessions += 1;
      inputTokens += v?.input_tokens ?? 0;
      outputTokens += v?.output_tokens ?? 0;
      cacheTokens += v?.cache_tokens ?? 0;
    }
    return { sessions, inputTokens, outputTokens, cacheTokens };
  }, [sessionUsage, usageOk]);

  const overviewData = useMemo(
    () =>
      (overview?.accounts ?? []).map((a: any) => ({
        name: a.displayName,
        sessions: a.sessionCount ?? 0,
      })),
    [overview],
  );

  const overviewConfig = { sessions: { label: "本地会话数", color: "var(--chart-3)" } } as const;

  return (
    <div className="mx-auto w-full min-w-0 max-w-5xl px-4 py-6 sm:px-6 sm:py-8">
      <header className="mb-6 flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Token 统计</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            按账号统计模型调用的输入 / 输出 / 缓存 token 用量与会话规模。
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Select value={accountId} onValueChange={setAccountId}>
            <SelectTrigger className="w-44">
              <SelectValue placeholder="选择账号" />
            </SelectTrigger>
            <SelectContent>
              {accounts.map((a) => (
                <SelectItem key={a.id} value={a.id}>
                  {a.displayName}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button variant="outline" size="icon" onClick={() => fetch(accountId)} disabled={loading}>
            <RefreshCw className={`size-4 ${loading ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </header>

      {loading && !data ? (
        <div className="grid gap-4 sm:grid-cols-3">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="h-28 rounded-xl" />
          ))}
        </div>
      ) : (
        <>
          {/* 顶部统计卡 */}
          <div className="mb-4 grid gap-4 sm:grid-cols-3">
            <Card>
              <CardHeader className="pb-2">
                <CardDescription>本地会话数</CardDescription>
                <CardTitle className="text-2xl">
                  {sessionStats?.sessionCount ?? "—"}
                </CardTitle>
              </CardHeader>
              <CardContent className="text-xs text-muted-foreground">
                来自 state.vscdb 的草稿会话记录
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardDescription>云端会话数</CardDescription>
                <CardTitle className="text-2xl">
                  {sessionUsageData?.sessions ?? (usageOk ? 0 : "—")}
                </CardTitle>
              </CardHeader>
              <CardContent>
                {usageOk ? (
                  <Badge variant="default" className="gap-1">
                    <CheckCircle2 className="size-3" /> 实时
                  </Badge>
                ) : (
                  <Badge variant="outline" className="gap-1 text-amber-600">
                    <AlertCircle className="size-3" /> {usageErr || "需刷新令牌"}
                  </Badge>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardDescription>总 Token 用量</CardDescription>
                <CardTitle className="text-2xl">
                  {sessionUsageData
                    ? (
                        sessionUsageData.inputTokens +
                        sessionUsageData.outputTokens +
                        sessionUsageData.cacheTokens
                      ).toLocaleString()
                    : "—"}
                </CardTitle>
              </CardHeader>
              <CardContent className="text-xs text-muted-foreground">
                输入 + 输出 + 缓存命中
              </CardContent>
            </Card>
          </div>

          {/* 用量明细 */}
          <Card>
            <CardHeader>
              <CardTitle>Token 用量明细</CardTitle>
              <CardDescription>按会话聚合的输入 / 输出 / 缓存 token</CardDescription>
            </CardHeader>
            <CardContent>
              {sessionUsageData ? (
                <div className="grid gap-2 sm:grid-cols-3">
                  <div className="rounded-lg border px-3 py-2.5">
                    <div className="text-xs text-muted-foreground">输入</div>
                    <div className="text-lg font-semibold">
                      {sessionUsageData.inputTokens.toLocaleString()}
                    </div>
                  </div>
                  <div className="rounded-lg border px-3 py-2.5">
                    <div className="text-xs text-muted-foreground">输出</div>
                    <div className="text-lg font-semibold">
                      {sessionUsageData.outputTokens.toLocaleString()}
                    </div>
                  </div>
                  <div className="rounded-lg border px-3 py-2.5">
                    <div className="text-xs text-muted-foreground">缓存命中</div>
                    <div className="text-lg font-semibold">
                      {sessionUsageData.cacheTokens.toLocaleString()}
                    </div>
                  </div>
                </div>
              ) : (
                <div className="flex flex-col items-center gap-2 py-8 text-center">
                  <MessagesSquare className="size-6 text-muted-foreground/50" />
                  <p className="text-sm text-muted-foreground">
                    云端用量接口当前需 IDE 运行时令牌才能访问。
                    <br />
                    本地统计：共 {sessionStats?.sessionCount ?? 0} 个会话
                    {sessionStats?.lastActiveMs ? `，最近活跃 ${fmtTs(sessionStats.lastActiveMs)}` : ""}
                  </p>
                </div>
              )}
            </CardContent>
          </Card>

          {/* 跨账号会话对比 */}
          {overviewData.length > 0 && (
            <Card className="mt-4">
              <CardHeader>
                <CardTitle>跨账号会话对比</CardTitle>
                <CardDescription>各账号快照中的本地草稿会话数</CardDescription>
              </CardHeader>
              <CardContent>
                <ChartContainer config={overviewConfig} className="h-48 w-full">
                  <BarChart data={overviewData} margin={{ top: 4, right: 4, bottom: 0, left: 0 }}>
                    <CartesianGrid vertical={false} strokeDasharray="3 3" />
                    <XAxis dataKey="name" tickLine={false} axisLine={false} tickMargin={8} fontSize={11} />
                    <YAxis tickLine={false} axisLine={false} fontSize={11} width={36} allowDecimals={false} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Bar dataKey="sessions" fill="var(--color-sessions)" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ChartContainer>
              </CardContent>
            </Card>
          )}
        </>
      )}
    </div>
  );
}
