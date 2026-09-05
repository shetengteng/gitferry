import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  AppStatePayload,
  ConflictSide,
  Direction,
  Platform,
  RepoEntry,
  RepoSyncResult,
  RepoSyncState,
  Settings,
} from "./types";

export function getAppState(): Promise<AppStatePayload> {
  return invoke("get_app_state");
}

export function scanRepos(): Promise<RepoEntry[]> {
  return invoke("scan_repos");
}

export function setRepoConfig(
  path: string,
  enabled: boolean,
  direction: Direction,
): Promise<void> {
  return invoke("set_repo_config", { path, enabled, direction });
}

export function setReposConfig(
  items: { path: string; enabled: boolean; direction: Direction }[],
): Promise<void> {
  return invoke("set_repos_config", { items });
}

export function saveSettings(
  scanRoots: string[],
  maxDepth: number,
  syncIntervalMins: number,
  concurrency: number,
): Promise<Settings> {
  return invoke("save_settings", {
    scanRoots,
    maxDepth,
    syncIntervalMins,
    concurrency,
  });
}

export function revealLogsDir(): Promise<void> {
  return invoke("reveal_logs_dir");
}

export function configureAccount(
  platform: Platform,
  token: string,
): Promise<Account> {
  return invoke("configure_account", { platform, token });
}

export function completeSetup(): Promise<void> {
  return invoke("complete_setup");
}

export function syncNow(path: string): Promise<RepoSyncState> {
  return invoke("sync_now", { path });
}

export function syncAll(): Promise<RepoSyncResult[]> {
  return invoke("sync_all");
}

export function getSyncStates(): Promise<RepoSyncResult[]> {
  return invoke("get_sync_states");
}

export function resolveConflict(
  path: string,
  side: ConflictSide,
): Promise<RepoSyncState> {
  return invoke("resolve_conflict", { path, side });
}

export function setRepoMirror(path: string, mirrorDelete: boolean): Promise<void> {
  return invoke("set_repo_mirror", { path, mirrorDelete });
}
