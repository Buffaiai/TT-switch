import { useEffect, useMemo, useState } from "react";
import { Sparkles, RefreshCw, AlertCircle, CheckCircle2, CalendarCheck } from "lucide-react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { toast } from "sonner";

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
import { api, type CheckinSummary } from "@/lib/api";
import { describeCheckin } from "@/lib/checkin";
import { useVisibleInterval } from "@/lib/use-visible-interval";

interface Activity {
  activityId: string;
  activityType: number;
  startTime: number;
  endTime: number;
  enabled: boolean;
  workExtra?: {
    general_credits?: number;
    work_credits?: number;
    granted?: boolean;
    grantable?: boolean;
    eligible?: boolean;
  };
}

interface CachedServerData {
  entitlementInfo?: {
    identityStr?: string;
    identity?: number;
    isPayFreshman?: boolean;
    enableSoloBuilder?: boolean;
    enableSoloCoder?: boolean;
  };
  originPayStatusData?: {
    is_credits_billing?: boolean;
    user_pay_identity_str?: string;
    is_pay_freshman?: boolean;
    server_time_ms?: number;
  };
  commercialActivityInfo?: {
    activities?: Activity[];
    lastUpdateTime?: number;
  };
}

function fmtTs(ms?: number): string {
  if (!ms) return "—";
  try {
    return new Date(ms).toLocaleString("zh-CN");
  } catch {
    return String(ms);
  }
}

const ACTIVITY_LABELS: Record<string, string> = {
  checkin_credits: "每日签到",
  new_user_credits: "新人积分",
  send_message_reward: "发消息奖励",
  student_auth_reward: "学生认证",
  work_desktop_download: "桌面版下载",
  work_fission_202607: "邀请裂变",
  code_fission_202609: "代码裂变",
  express_lottery: "极速抽奖",
  work_lite_202606: "Lite 专享",
};

function activityLabel(id: string): string {
  return ACTIVITY_LABELS[id] || id;
}

export default function CreditStatsPage() {
  const { accounts } = useAccountsStore();
  const [accountId, setAccountId] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [data, setData] = useState<any>(null);
  const [checkin, setCheckin] = useState<CheckinSummary | null>(null);
  const [checkinRunning, setCheckinRunning] = useState(false);

  useEffect(() => {
    if (accounts.length && !accountId) {
      setAccountId(accounts[0].id);
    }
  }, [accounts, accountId]);

  const fetch = async (id?: string) => {
    setLoading(true);
    try {
      const res = await api.getCommercialStats(id || undefined);
      setData(res);
    } catch {
      setData(null);
    } finally {
      setLoading(false);
    }
  };

  const fetchCheckin = async () => {
    try {
      setCheckin(await api.getCheckinStatus());
    } catch {
      /* 状态不可用时保持上一次数据 */
    }
  };

  useEffect(() => {
    if (accountId) fetch(accountId);
  }, [accountId]);

  // 可见时每 60s 刷新签到状态与积分数据；隐藏时暂停。
  useVisibleInterval(
    () => {
      fetchCheckin();
      if (accountId) fetch(accountId);
    },
    60_000,
    Boolean(accountId),
  );

  const runCheckin = async () => {
    setCheckinRunning(true);
    try {
      const res = await api.runCheckinNow();
      const okCount = (res.results ?? []).filter(
        (r) => r.result === "ok" || r.result === "already",
      ).length;
      const failCount = (res.results ?? []).filter((r) => r.result === "error").length;
      if (res.ok) {
        toast.success(`签到完成：${okCount} 个成功${failCount ? `，${failCount} 个失败` : ""}`);
      } else {
        toast.error(res.error || "签到失败");
      }
      await fetchCheckin();
      if (accountId) await fetch(accountId);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "签到失败");
    } finally {
      setCheckinRunning(false);
    }
  };

  const cached: CachedServerData | null = data?.cachedServerData ?? null;
  const balanceOk = data?.balance?.ok;
  const balanceErr = data?.balance?.error;

  const activities = useMemo<Activity[]>(
    () => cached?.commercialActivityInfo?.activities ?? [],
    [cached],
  );

  const totalCredits = useMemo(() => {
    let sum = 0;
    for (const a of activities) {
      sum += a.workExtra?.general_credits ?? 0;
      sum += a.workExtra?.work_credits ?? 0;
    }
    return sum;
  }, [activities]);

  const chartData = useMemo(
    () =>
      activities.map((a) => ({
        name: activityLabel(a.activityId),
        general: a.workExtra?.general_credits ?? 0,
        work: a.workExtra?.work_credits ?? 0,
      })),
    [activities],
  );

  const chartConfig = {
    general: { label: "通用积分", color: "var(--chart-1)" },
    work: { label: "Work 积分", color: "var(--chart-2)" },
  } as const;

  return (
    <div className="mx-auto w-full min-w-0 max-w-5xl px-4 py-6 sm:px-6 sm:py-8">
      <header className="mb-6 flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">积分统计</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            积分余额、权益档位与活动奖励一览，数据来自本机 IDE 缓存 + 云端商业接口。
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
          {/* 每日签到卡：当前选中账号的今日状态 + 手动触发全账号签到 */}
          <Card className="mb-4">
            <CardContent className="flex flex-wrap items-center gap-3 py-4">
              <CalendarCheck className="size-5 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium">每日签到</span>
                  {(() => {
                    const d = describeCheckin(
                      checkin?.accounts?.[accountId],
                      checkin?.today ?? "",
                    );
                    return (
                      <Badge
                        variant={
                          d.tone === "success"
                            ? "default"
                            : d.tone === "error"
                              ? "destructive"
                              : "outline"
                        }
                      >
                        {d.text}
                      </Badge>
                    );
                  })()}
                  {typeof checkin?.accounts?.[accountId]?.last_credits === "number" && (
                    <span className="text-sm text-muted-foreground">
                      签到积分 {checkin.accounts[accountId].last_credits}
                    </span>
                  )}
                </div>
                <p className="mt-0.5 text-xs text-muted-foreground">
                  {checkin?.enabled
                    ? `自动签到已开启 · 每日 ${checkin.time}（覆盖全部已保存账号）`
                    : "自动签到已关闭，可在设置中开启"}
                </p>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={runCheckin}
                disabled={checkinRunning}
                className="gap-1.5"
              >
                <RefreshCw className={`size-3.5 ${checkinRunning ? "animate-spin" : ""}`} />
                {checkinRunning ? "签到中…" : "立即签到"}
              </Button>
            </CardContent>
          </Card>

          {/* 顶部汇总卡 */}
          <div className="mb-4 grid gap-4 sm:grid-cols-3">
            <Card>
              <CardHeader className="pb-2">
                <CardDescription>当前档位</CardDescription>
                <CardTitle className="text-2xl">
                  {cached?.entitlementInfo?.identityStr ?? "—"}
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="flex flex-wrap gap-1.5">
                  {cached?.entitlementInfo?.enableSoloBuilder && (
                    <Badge variant="secondary">Solo Builder</Badge>
                  )}
                  {cached?.entitlementInfo?.enableSoloCoder && (
                    <Badge variant="secondary">Solo Coder</Badge>
                  )}
                  {cached?.originPayStatusData?.is_credits_billing && (
                    <Badge>积分计费</Badge>
                  )}
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardDescription>活动积分总额</CardDescription>
                <CardTitle className="text-2xl">{totalCredits.toLocaleString()}</CardTitle>
              </CardHeader>
              <CardContent className="text-xs text-muted-foreground">
                来自 {activities.length} 个活动的 general + work 积分
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardDescription>云端余额</CardDescription>
                <CardTitle className="flex items-center gap-2 text-2xl">
                  {balanceOk ? "已连接" : balanceErr ? "未获取" : "—"}
                </CardTitle>
              </CardHeader>
              <CardContent>
                {balanceOk ? (
                  <Badge variant="default" className="gap-1">
                    <CheckCircle2 className="size-3" /> 实时
                  </Badge>
                ) : (
                  <Badge variant="outline" className="gap-1 text-amber-600">
                    <AlertCircle className="size-3" /> {balanceErr || "需刷新令牌"}
                  </Badge>
                )}
              </CardContent>
            </Card>
          </div>

          {/* 活动积分图表 */}
          {chartData.length > 0 && (
            <Card className="mb-4">
              <CardHeader>
                <CardTitle>活动积分分布</CardTitle>
                <CardDescription>各活动的通用积分与 Work 积分</CardDescription>
              </CardHeader>
              <CardContent>
                <ChartContainer config={chartConfig} className="h-56 w-full">
                  <BarChart data={chartData} margin={{ top: 4, right: 4, bottom: 0, left: 0 }}>
                    <CartesianGrid vertical={false} strokeDasharray="3 3" />
                    <XAxis
                      dataKey="name"
                      tickLine={false}
                      axisLine={false}
                      tickMargin={8}
                      fontSize={11}
                      interval={0}
                      angle={-20}
                      height={48}
                      textAnchor="end"
                    />
                    <YAxis tickLine={false} axisLine={false} fontSize={11} width={40} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Bar dataKey="general" stackId="a" fill="var(--color-general)" radius={[0, 0, 2, 2]} />
                    <Bar dataKey="work" stackId="a" fill="var(--color-work)" radius={[2, 2, 0, 0]} />
                  </BarChart>
                </ChartContainer>
              </CardContent>
            </Card>
          )}

          {/* 活动明细 */}
          <Card>
            <CardHeader>
              <CardTitle>活动奖励</CardTitle>
              <CardDescription>
                各商业活动的积分额度与领取状态
                {cached?.commercialActivityInfo?.lastUpdateTime && (
                  <>
                    {" · 缓存更新于 "}
                    {fmtTs(cached.commercialActivityInfo.lastUpdateTime)}
                  </>
                )}
              </CardDescription>
            </CardHeader>
            <CardContent>
              {activities.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无活动数据</p>
              ) : (
                <div className="grid gap-2 sm:grid-cols-2">
                  {activities.map((a) => (
                    <div
                      key={a.activityId}
                      className="flex items-center justify-between rounded-lg border px-3 py-2.5"
                    >
                      <div>
                        <div className="text-sm font-medium">{activityLabel(a.activityId)}</div>
                        <div className="text-xs text-muted-foreground">
                          {a.enabled ? "进行中" : "已结束"} · 至{" "}
                          {fmtTs(a.endTime)}
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="text-sm font-semibold">
                          {(a.workExtra?.general_credits ?? 0) +
                            (a.workExtra?.work_credits ?? 0)}
                        </div>
                        <div className="text-xs text-muted-foreground">
                          {a.workExtra?.granted ? "已领取" : "未领取"}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>

          {!balanceOk && (
            <p className="mt-4 flex items-start gap-2 text-xs text-muted-foreground">
              <Sparkles className="mt-0.5 size-3.5 shrink-0" />
              云端积分接口当前需 IDE 运行时的 marscode 令牌才能访问；上方展示的是 IDE 本地缓存的权益与活动数据，
              切换账号后数据会随对应 IDE 刷新更新。
            </p>
          )}
        </>
      )}
    </div>
  );
}
