import { defineStore } from "pinia";
import * as api from "@/lib/invoke";
import type {
  Account,
  ConflictSide,
  Direction,
  Platform,
  RepoEntry,
  RepoSyncResult,
  RepoSyncState,
  Settings,
} from "@/lib/types";

interface AppState {
  loaded: boolean;
  setupDone: boolean;
  repos: RepoEntry[];
  settings: Settings;
  accounts: Record<Platform, Account>;
  scanning: boolean;
  scanError: string | null;
  query: string;
  syncStates: Record<string, RepoSyncState>;
  syncingAll: boolean;
  syncingError: string | null;
  /** 最近一次全量同步成功的时间（毫秒时间戳），null 表示尚未全量同步过 */
  lastSyncAllAt: number | null;
}

export interface RepoStats {
  synced: number;
  pending: number;
  conflict: number;
  error: number;
}

export interface RepoKindStats {
  github: number;
  gitee: number;
  both: number;
}

function resultsToStates(results: RepoSyncResult[]): Record<string, RepoSyncState> {
  const map: Record<string, RepoSyncState> = {};
  for (const r of results) map[r.path] = r.state;
  return map;
}

export const useAppStore = defineStore("app", {
  state: (): AppState => ({
    loaded: false,
    setupDone: false,
    repos: [],
    settings: { scan_roots: [], max_depth: 4, sync_interval_mins: 10, concurrency: 2 },
    accounts: {
      github: { status: "unconfigured", login: null },
      gitee: { status: "unconfigured", login: null },
    },
    scanning: false,
    scanError: null,
    query: "",
    syncStates: {},
    syncingAll: false,
    syncingError: null,
    lastSyncAllAt: null,
  }),
  getters: {
    enabledCount: (s) => s.repos.filter((r) => r.enabled).length,
    visibleRepos: (s) => {
      const q = s.query.trim().toLowerCase();
      if (!q) return s.repos;
      return s.repos.filter(
        (r) =>
          r.path.toLowerCase().includes(q) ||
          r.remotes.some((m) => m.url.toLowerCase().includes(q)),
      );
    },
    syncStateOf: (s) => {
      return (path: string): RepoSyncState | undefined => s.syncStates[path];
    },
    conflictCount: (s) =>
      Object.values(s.syncStates).filter((st) => st.status === "conflict").length,
    errorCount: (s) =>
      Object.values(s.syncStates).filter((st) => st.status === "error").length,
    /** 全部仓库中最近一次同步成功的时间（unix 秒），无任何记录时为 null */
    lastSyncedAt: (s): number | null => {
      let max: number | null = null;
      for (const st of Object.values(s.syncStates)) {
        if (st.last_synced !== null && (max === null || st.last_synced > max)) {
          max = st.last_synced;
        }
      }
      return max;
    },
    /** 启用仓库按状态计数（无同步记录视作待同步），用于主界面指标卡 */
    stats: (s): RepoStats => {
      const stats: RepoStats = { synced: 0, pending: 0, conflict: 0, error: 0 };
      for (const repo of s.repos) {
        if (!repo.enabled) continue;
        const status = s.syncStates[repo.path]?.status ?? "pending";
        if (status === "synced") stats.synced++;
        else if (status === "conflict") stats.conflict++;
        else if (status === "error" || status === "missing") stats.error++;
        else stats.pending++;
      }
      return stats;
    },
    /** 按 remote 归属计数（不含无 remote 仓库），用于向导扫描结果卡 */
    kindStats: (s): RepoKindStats => {
      const stats: RepoKindStats = { github: 0, gitee: 0, both: 0 };
      for (const repo of s.repos) {
        if (repo.kind === "github_only") stats.github++;
        else if (repo.kind === "gitee_only") stats.gitee++;
        else if (repo.kind === "both") stats.both++;
      }
      return stats;
    },
  },
  actions: {
    async init() {
      const [state] = await Promise.all([
        api.getAppState(),
        api
          .getSyncStates()
          .then((results) => {
            this.syncStates = resultsToStates(results);
          })
          .catch(() => {
            // 同步状态获取失败不阻塞初始化，单仓库操作时会按需刷新
          }),
      ]);
      this.setupDone = state.setup_done;
      this.repos = state.repos;
      this.settings = state.settings;
      this.accounts = state.accounts;
      this.loaded = true;
    },
    async scan() {
      this.scanning = true;
      this.scanError = null;
      try {
        this.repos = await api.scanRepos();
      } catch (err) {
        this.scanError = String(err);
      } finally {
        this.scanning = false;
      }
    },
    async setRepo(path: string, enabled: boolean, direction: Direction) {
      await api.setRepoConfig(path, enabled, direction);
      const repo = this.repos.find((r) => r.path === path);
      if (repo) {
        repo.enabled = enabled;
        repo.direction = direction;
      }
    },
    async setRepos(
      items: { path: string; enabled: boolean; direction: Direction }[],
    ) {
      await api.setReposConfig(items);
      const byPath = new Map(items.map((i) => [i.path, i]));
      for (const repo of this.repos) {
        const item = byPath.get(repo.path);
        if (item) {
          repo.enabled = item.enabled;
          repo.direction = item.direction;
        }
      }
    },
    async saveSettings(
      scanRoots: string[],
      maxDepth: number,
      syncIntervalMins: number,
      concurrency: number,
    ) {
      this.settings = await api.saveSettings(
        scanRoots,
        maxDepth,
        syncIntervalMins,
        concurrency,
      );
    },
    async addScanRoot(root: string) {
      const roots = [...new Set([...this.settings.scan_roots, root])];
      await this.saveSettings(
        roots,
        this.settings.max_depth,
        this.settings.sync_interval_mins,
        this.settings.concurrency,
      );
    },
    async removeScanRoot(root: string) {
      const roots = this.settings.scan_roots.filter((r) => r !== root);
      await this.saveSettings(
        roots,
        this.settings.max_depth,
        this.settings.sync_interval_mins,
        this.settings.concurrency,
      );
    },
    async setMaxDepth(maxDepth: number) {
      await this.saveSettings(
        [...this.settings.scan_roots],
        maxDepth,
        this.settings.sync_interval_mins,
        this.settings.concurrency,
      );
    },
    async setSyncInterval(mins: number) {
      await this.saveSettings(
        [...this.settings.scan_roots],
        this.settings.max_depth,
        mins,
        this.settings.concurrency,
      );
    },
    async setConcurrency(concurrency: number) {
      await this.saveSettings(
        [...this.settings.scan_roots],
        this.settings.max_depth,
        this.settings.sync_interval_mins,
        concurrency,
      );
    },
    async revealLogs() {
      await api.revealLogsDir();
    },
    async configureAccount(platform: Platform, token: string) {
      const account = await api.configureAccount(platform, token);
      this.accounts[platform] = account;
    },
    async completeSetup() {
      await api.completeSetup();
      this.setupDone = true;
    },
    async syncRepo(path: string) {
      const prev = this.syncStates[path];
      this.syncStates[path] = {
        status: "syncing",
        last_synced: prev?.last_synced ?? null,
        pushed_refs: prev?.pushed_refs ?? 0,
        error: null,
        conflicts: prev?.conflicts ?? [],
      };
      try {
        this.syncStates[path] = await api.syncNow(path);
      } catch (err) {
        this.syncStates[path] = {
          status: "error",
          last_synced: prev?.last_synced ?? null,
          pushed_refs: prev?.pushed_refs ?? 0,
          error: String(err),
          conflicts: prev?.conflicts ?? [],
        };
      }
    },
    async syncAll() {
      this.syncingAll = true;
      this.syncingError = null;
      for (const repo of this.repos) {
        if (!repo.enabled || repo.kind === "none") continue;
        const prev = this.syncStates[repo.path];
        this.syncStates[repo.path] = {
          status: "syncing",
          last_synced: prev?.last_synced ?? null,
          pushed_refs: prev?.pushed_refs ?? 0,
          error: null,
          conflicts: prev?.conflicts ?? [],
        };
      }
      try {
        this.syncStates = resultsToStates(await api.syncAll());
        this.lastSyncAllAt = Date.now();
      } catch (err) {
        this.syncingError = String(err);
      } finally {
        this.syncingAll = false;
      }
    },
    async resolveConflict(path: string, side: ConflictSide) {
      const prev = this.syncStates[path];
      this.syncStates[path] = {
        status: "syncing",
        last_synced: prev?.last_synced ?? null,
        pushed_refs: prev?.pushed_refs ?? 0,
        error: null,
        conflicts: prev?.conflicts ?? [],
      };
      try {
        this.syncStates[path] = await api.resolveConflict(path, side);
      } catch (err) {
        // 裁决失败：恢复原状态（保持 conflict 可重试），错误抛给弹窗展示
        if (prev) {
          this.syncStates[path] = prev;
        } else {
          delete this.syncStates[path];
        }
        throw err;
      }
    },
  },
});
