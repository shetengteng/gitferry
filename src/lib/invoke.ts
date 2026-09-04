import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  AppStatePayload,
  Direction,
  Platform,
  RepoEntry,
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
): Promise<Settings> {
  return invoke("save_settings", { scanRoots, maxDepth });
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
