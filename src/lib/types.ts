export type ClientKind = "ide_cn" | "solo_cn" | "ide_intl";

export interface ClientStatus {
  client: ClientKind;
  clientName: string;
  installed: boolean;
  appPath: string | null;
  running: boolean;
  loggedIn: boolean;
  currentUid: string | null;
  expiresAt: number | null;
}

export interface AppStatus {
  version: string;
  clients: ClientStatus[];
  accountCount: number;
}

export interface AccountMeta {
  id: string;
  uid: string | null;
  nickname: string | null;
  displayName: string;
  client: ClientKind;
  clientName: string | null;
  createdAt: number | null;
  capturedAt: number | null;
  expiresAt: number | null;
  needsRelogin: boolean;
}

export interface AccountsPayload {
  accounts: AccountMeta[];
}

export interface SwitchProgressMessage {
  at: number;
  message: string;
}

export interface SwitchProgress {
  running: boolean;
  messages: SwitchProgressMessage[];
}

export interface SwitchResult {
  ok: boolean;
  client: ClientKind;
  restarted: boolean;
  alreadyCurrent: boolean;
}

export interface PermissionCheck {
  ok: boolean;
  client: ClientKind;
  message?: string;
  error?: string;
  dir?: string;
  hint?: string;
}

export interface BackupInfo {
  name: string;
  atMs: number;
  sizeBytes: number;
}

export interface SystemInfo {
  storeDir: string;
  backupsDir: string;
  backups: BackupInfo[];
  clientDataDirs: {
    client: ClientKind;
    clientName: string;
    dir: string;
    agentToken: string;
    exists: boolean;
  }[];
}

export interface ErrorLogEntry {
  at: number;
  kind: string;
  message: string;
}

export interface TransferPreviewItem extends AccountMeta {
  snapshotFiles: number;
  conflict: "sameId" | "sameAccount" | null;
}

export interface TransferPreview {
  path: string;
  exportedAt: number | null;
  accounts: TransferPreviewItem[];
}

export interface TransferResult {
  imported: AccountMeta[];
  skipped: { displayName?: string; reason: string }[];
}

export interface ExportResult {
  path: string;
  count: number;
  sizeBytes: number;
  accounts: AccountMeta[];
}
