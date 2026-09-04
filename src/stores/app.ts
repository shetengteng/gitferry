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
    async saveSettings(scanRoots: string[], maxDepth: number) {
      this.settings = await api.saveSettings(scanRoots, maxDepth);
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
