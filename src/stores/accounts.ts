import { create } from "zustand";

import { api } from "@/lib/api";
import type {
  AccountMeta,
  AppStatus,
  PermissionCheck,
  SwitchProgress,
  SystemInfo,
} from "@/lib/types";

interface AccountsState {
  status: AppStatus | null;
  accounts: AccountMeta[];
  systemInfo: SystemInfo | null;
  checks: PermissionCheck[];
  progress: SwitchProgress | null;
  loading: boolean;
  error: string | null;
  /** 全量刷新：状态 + 账号 + 系统信息。 */
  refresh: () => Promise<void>;
  /** 仅刷新切换进度（轮询用）；服务未起时静默返回 null。 */
  refreshProgress: () => Promise<SwitchProgress | null>;
  upsertCheck: (check: PermissionCheck) => void;
}

export const useAccountsStore = create<AccountsState>((set) => ({
  status: null,
  accounts: [],
  systemInfo: null,
  checks: [],
  progress: null,
  loading: false,
  error: null,

  refresh: async () => {
    set({ loading: true, error: null });
    try {
      const [status, payload, systemInfo] = await Promise.all([
        api.getStatus(),
        api.getAccounts(),
        api.getSystemInfo(),
      ]);
      set({ status, accounts: payload.accounts, systemInfo });
    } catch (cause) {
      const error = cause instanceof Error ? cause.message : String(cause);
      set({ error });
      throw cause;
    } finally {
      set({ loading: false });
    }
  },

  refreshProgress: async () => {
    try {
      const progress = await api.switchProgress();
      set({ progress });
      return progress;
    } catch {
      return null;
    }
  },

  upsertCheck: (check) =>
    set((state) => ({
      checks: [...state.checks.filter((x) => x.client !== check.client), check],
    })),
}));
