import { defineStore } from "pinia";
import * as api from "@/lib/invoke";
import type {
  Account,
  AppStatePayload,
  Direction,
  Platform,
  RepoEntry,
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
}

export const useAppStore = defineStore("app", {
  state: (): AppState => ({
    loaded: false,
    setupDone: false,
    repos: [],
    settings: { scan_roots: [], max_depth: 4 },
    accounts: {
      github: { status: "unconfigured", login: null },
      gitee: { status: "unconfigured", login: null },
    },
    scanning: false,
    scanError: null,
    query: "",
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
  },
  actions: {
    async init() {
      const state: AppStatePayload = await api.getAppState();
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
    async saveSettings(scanRoots: string[], maxDepth: number) {
      this.settings = await api.saveSettings(scanRoots, maxDepth);
    },
    async addScanRoot(root: string) {
      const roots = [...new Set([...this.settings.scan_roots, root])];
      await this.saveSettings(roots, this.settings.max_depth);
    },
    async removeScanRoot(root: string) {
      const roots = this.settings.scan_roots.filter((r) => r !== root);
      await this.saveSettings(roots, this.settings.max_depth);
    },
    async setMaxDepth(maxDepth: number) {
      await this.saveSettings([...this.settings.scan_roots], maxDepth);
    },
    async configureAccount(platform: Platform, token: string) {
      const account = await api.configureAccount(platform, token);
      this.accounts[platform] = account;
    },
    async completeSetup() {
      await api.completeSetup();
      this.setupDone = true;
    },
  },
});
