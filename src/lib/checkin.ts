/**
 * 每日签到状态展示辅助：把 checkin.json 的 per-账号记录映射为徽章文案与色调。
 * CreditStatsPage 签到卡片与 account-card 积分徽章共用。
 */
import type { CheckinAccountRec } from "./api";

export type CheckinTone = "success" | "warn" | "error" | "muted";

export interface CheckinDisplay {
  /** 徽章短文案（如「已签」「未签」「需重新登录」）。 */
  text: string;
  tone: CheckinTone;
  /** 积分值（有则展示）。 */
  credits: number | null;
}

export function describeCheckin(
  rec: CheckinAccountRec | undefined,
  today: string,
): CheckinDisplay {
  const credits = typeof rec?.last_credits === "number" ? rec.last_credits : null;
  if (!rec || rec.last_claim_date !== today) {
    return { text: "未签到", tone: "muted", credits };
  }
  switch (rec.last_result) {
    case "ok":
      return { text: "已签到", tone: "success", credits };
    case "already":
      return { text: "已签到", tone: "success", credits };
    case "error":
      if (rec.last_error === "token_expired" || rec.last_error === "no_token") {
        return { text: "需重新登录", tone: "error", credits };
      }
      if (rec.last_error === "activity_disabled") {
        return { text: "活动未开启", tone: "muted", credits };
      }
      return { text: "签到失败", tone: "warn", credits };
    default:
      return { text: "未签到", tone: "muted", credits };
  }
}
