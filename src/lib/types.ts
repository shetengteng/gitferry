export type Direction = "github_to_gitee" | "gitee_to_github" | "both";

export type HostKind = "github" | "gitee" | "other";

export type RepoKind = "github_only" | "gitee_only" | "both" | "none";

export type AccountStatus = "unconfigured" | "connected" | "invalid";

export interface RemoteInfo {
  name: string;
  url: string;
  host: HostKind;
}

export interface RepoEntry {
  path: string;
  enabled: boolean;
  direction: Direction;
  remotes: RemoteInfo[];
  kind: RepoKind;
}

export interface Settings {
  scan_roots: string[];
  max_depth: number;
  /** 自动同步轮询间隔（分钟），调度器（M3）消费 */
  sync_interval_mins: number;
  /** 并发同步仓库数上限，调度器（M3）消费 */
  concurrency: number;
}

export interface Account {
  status: AccountStatus;
  login: string | null;
}

export interface Accounts {
  github: Account;
  gitee: Account;
}

export interface AppStatePayload {
  settings: Settings;
  repos: RepoEntry[];
  accounts: Accounts;
  setup_done: boolean;
}

export type Platform = "github" | "gitee";

export type SyncStatus =
  | "pending"
  | "syncing"
  | "synced"
  | "conflict"
  | "error"
  | "missing";

export type RefKind = "branch" | "tag";

export type ConflictSide = "github" | "gitee";

export interface RefConflict {
  kind: RefKind;
  name: string;
  github_sha: string;
  gitee_sha: string;
}

export interface RepoSyncState {
  status: SyncStatus;
  last_synced: number | null;
  pushed_refs: number;
  error: string | null;
  conflicts: RefConflict[];
}

export interface RepoSyncResult {
  path: string;
  state: RepoSyncState;
}
