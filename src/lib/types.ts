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
