import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppStore } from "./app";
import type {
  AppStatePayload,
  RepoEntry,
  RepoSyncResult,
  RepoSyncState,
} from "@/lib/types";

vi.mock("@/lib/invoke", () => ({
  getAppState: vi.fn(),
  scanRepos: vi.fn(),
  setRepoConfig: vi.fn().mockResolvedValue(undefined),
  setReposConfig: vi.fn().mockResolvedValue(undefined),
  setRepoMirror: vi.fn(),
  saveSettings: vi.fn(),
  revealLogsDir: vi.fn().mockResolvedValue(undefined),
  configureAccount: vi.fn(),
  completeSetup: vi.fn().mockResolvedValue(undefined),
  getSyncStates: vi.fn().mockResolvedValue([]),
  syncNow: vi.fn(),
  syncAll: vi.fn(),
  resolveConflict: vi.fn(),
}));

vi.mock("@/lib/events", () => ({
  listenSyncStates: vi.fn(async () => () => {}),
}));

import * as api from "@/lib/invoke";
import * as eventsApi from "@/lib/events";

function repo(path: string, overrides: Partial<RepoEntry> = {}): RepoEntry {
  return {
    path,
    enabled: false,
    direction: "both",
    remotes: [],
    kind: "both",
    mirror_delete: false,
    ...overrides,
  };
}

function syncState(overrides: Partial<RepoSyncState> = {}): RepoSyncState {
  return {
    status: "pending",
    last_synced: null,
    pushed_refs: 0,
    deleted_refs: 0,
    error: null,
    conflicts: [],
    ...overrides,
  };
}

function appState(overrides: Partial<AppStatePayload> = {}): AppStatePayload {
  return {
    settings: { scan_roots: ["~"], max_depth: 4, sync_interval_mins: 10, concurrency: 2 },
    repos: [],
    accounts: {
      github: { status: "unconfigured", login: null },
      gitee: { status: "unconfigured", login: null },
    },
    setup_done: true,
    ...overrides,
  };
}

const mockedApi = vi.mocked(api);
const mockedEvents = vi.mocked(eventsApi);

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
  mockedApi.getSyncStates.mockResolvedValue([]);
});

describe("setRepoMirror", () => {
  it("成功时更新本地仓库 mirror_delete", async () => {
    const store = useAppStore();
    store.repos = [repo("~/a/x"), repo("~/b/y")];
    mockedApi.setRepoMirror.mockResolvedValueOnce(undefined);
    await store.setRepoMirror("~/a/x", true);
    expect(mockedApi.setRepoMirror).toHaveBeenCalledWith("~/a/x", true);
    expect(store.repos[0].mirror_delete).toBe(true);
    expect(store.repos[1].mirror_delete).toBe(false);
  });

  it("失败时抛出错误且不改本地状态", async () => {
    const store = useAppStore();
    store.repos = [repo("~/a/x")];
    mockedApi.setRepoMirror.mockRejectedValueOnce("保存配置失败：磁盘不可写");
    await expect(store.setRepoMirror("~/a/x", true)).rejects.toBe(
      "保存配置失败：磁盘不可写",
    );
    expect(store.repos[0].mirror_delete).toBe(false);
  });
});

describe("bindSyncEvents", () => {
  it("事件回调按 merge 语义更新 syncStates，其余仓库保留", async () => {
    const store = useAppStore();
    let fire: ((results: RepoSyncResult[]) => void) | undefined;
    mockedEvents.listenSyncStates.mockImplementationOnce(async (cb) => {
      fire = cb;
      return () => {};
    });
    mockedApi.getAppState.mockResolvedValueOnce(
      appState({ repos: [repo("~/a/x")] }),
    );
    mockedApi.getSyncStates.mockResolvedValueOnce([
      { path: "~/b/y", state: syncState({ status: "synced", last_synced: 1 }) },
    ]);
    await store.init();
    expect(fire).toBeTypeOf("function");
    fire!([{ path: "~/a/x", state: syncState({ status: "error", error: "push 被拒" }) }]);
    expect(store.syncStates["~/b/y"].status).toBe("synced");
    expect(store.syncStates["~/a/x"].status).toBe("error");
  });
});

describe("visibleRepos", () => {
  it("query 为空时返回全部仓库", () => {
    const store = useAppStore();
    store.repos = [repo("~/a/x"), repo("~/b/y")];
    expect(store.visibleRepos).toHaveLength(2);
  });

  it("按路径过滤", () => {
    const store = useAppStore();
    store.repos = [repo("~/a/x"), repo("~/b/y")];
    store.query = " /B/ ";
    expect(store.visibleRepos.map((r) => r.path)).toEqual(["~/b/y"]);
  });

  it("按 remote url 过滤", () => {
    const store = useAppStore();
    store.repos = [
      repo("~/a/x", {
        remotes: [{ name: "origin", url: "https://gitee.com/a/x.git", host: "gitee" }],
      }),
      repo("~/b/y"),
    ];
    store.query = "gitee.com";
    expect(store.visibleRepos.map((r) => r.path)).toEqual(["~/a/x"]);
  });
});

describe("setRepos", () => {
  it("批量更新本地状态并调用一次保存", async () => {
    const store = useAppStore();
    store.repos = [repo("~/a/x"), repo("~/b/y")];
    await store.setRepos([
      { path: "~/a/x", enabled: true, direction: "github_to_gitee" },
    ]);
    expect(mockedApi.setReposConfig).toHaveBeenCalledTimes(1);
    expect(store.repos[0].enabled).toBe(true);
    expect(store.repos[0].direction).toBe("github_to_gitee");
    expect(store.repos[1].enabled).toBe(false);
  });
});

describe("scan", () => {
  it("失败时错误落入 scanError，扫描态复位", async () => {
    const store = useAppStore();
    mockedApi.scanRepos.mockRejectedValueOnce("permission denied");
    await store.scan();
    expect(store.scanError).toBe("permission denied");
    expect(store.scanning).toBe(false);
  });

  it("成功时刷新仓库列表", async () => {
    const store = useAppStore();
    mockedApi.scanRepos.mockResolvedValueOnce([repo("~/a/x", { enabled: true })]);
    await store.scan();
    expect(store.repos).toHaveLength(1);
    expect(store.enabledCount).toBe(1);
    expect(store.scanning).toBe(false);
  });
});

describe("init", () => {
  it("并行拉取应用状态与同步状态", async () => {
    const store = useAppStore();
    mockedApi.getAppState.mockResolvedValueOnce(
      appState({ repos: [repo("~/a/x")], setup_done: true }),
    );
    mockedApi.getSyncStates.mockResolvedValueOnce([
      { path: "~/a/x", state: syncState({ status: "synced", last_synced: 1000 }) },
    ]);
    await store.init();
    expect(store.loaded).toBe(true);
    expect(store.repos).toHaveLength(1);
    expect(store.syncStates["~/a/x"].status).toBe("synced");
  });

  it("同步状态获取失败不阻塞初始化", async () => {
    const store = useAppStore();
    mockedApi.getAppState.mockResolvedValueOnce(appState());
    mockedApi.getSyncStates.mockRejectedValueOnce("读取状态失败");
    await store.init();
    expect(store.loaded).toBe(true);
    expect(store.syncStates).toEqual({});
  });
});

describe("syncRepo", () => {
  it("成功时写入后端返回的同步状态", async () => {
    const store = useAppStore();
    mockedApi.syncNow.mockResolvedValueOnce(
      syncState({ status: "synced", last_synced: 1700000000, pushed_refs: 3 }),
    );
    await store.syncRepo("~/a/x");
    expect(mockedApi.syncNow).toHaveBeenCalledWith("~/a/x");
    expect(store.syncStates["~/a/x"].status).toBe("synced");
    expect(store.syncStates["~/a/x"].last_synced).toBe(1700000000);
    expect(store.syncStates["~/a/x"].error).toBeNull();
  });

  it("失败时置 error 并保留旧 last_synced 与冲突", async () => {
    const store = useAppStore();
    store.syncStates["~/a/x"] = syncState({
      status: "synced",
      last_synced: 111,
      conflicts: [
        { kind: "branch", name: "main", github_sha: "aaaaaaa", gitee_sha: "bbbbbbb" },
      ],
    });
    mockedApi.syncNow.mockRejectedValueOnce("gitee push 被拒：需要先完成一次终端 push 以保存凭据");
    await store.syncRepo("~/a/x");
    expect(store.syncStates["~/a/x"].status).toBe("error");
    expect(store.syncStates["~/a/x"].error).toContain("gitee push 被拒");
    expect(store.syncStates["~/a/x"].last_synced).toBe(111);
    expect(store.syncStates["~/a/x"].conflicts).toHaveLength(1);
    expect(store.errorCount).toBe(1);
  });
});

describe("syncAll", () => {
  it("成功时整体刷新同步状态并复位 syncingAll", async () => {
    const store = useAppStore();
    store.repos = [repo("~/a/x", { enabled: true }), repo("~/b/y", { enabled: true })];
    mockedApi.syncAll.mockResolvedValueOnce([
      { path: "~/a/x", state: syncState({ status: "synced", last_synced: 222 }) },
      { path: "~/b/y", state: syncState({ status: "pending" }) },
    ]);
    await store.syncAll();
    expect(mockedApi.syncAll).toHaveBeenCalledTimes(1);
    expect(store.syncingAll).toBe(false);
    expect(store.syncingError).toBeNull();
    expect(store.syncStates["~/a/x"].status).toBe("synced");
    expect(store.syncStates["~/b/y"].status).toBe("pending");
    expect(store.lastSyncAllAt).not.toBeNull();
  });

  it("进行中先把启用仓库置 syncing，失败时错误落入 syncingError", async () => {
    const store = useAppStore();
    store.repos = [repo("~/a/x", { enabled: true }), repo("~/b/y")];
    let seenStatus: string | undefined;
    mockedApi.syncAll.mockImplementationOnce(() => {
      seenStatus = store.syncStates["~/a/x"]?.status;
      return Promise.reject("同步引擎不可用");
    });
    await store.syncAll();
    expect(seenStatus).toBe("syncing");
    expect(store.syncingAll).toBe(false);
    expect(store.syncingError).toBe("同步引擎不可用");
    expect(store.lastSyncAllAt).toBeNull();
  });
});

describe("saveSettings", () => {
  it("setSyncInterval 全量提交并回写设置", async () => {
    const store = useAppStore();
    store.settings = { scan_roots: ["~/dev"], max_depth: 3, sync_interval_mins: 10, concurrency: 2 };
    mockedApi.saveSettings.mockResolvedValueOnce({
      scan_roots: ["~/dev"],
      max_depth: 3,
      sync_interval_mins: 30,
      concurrency: 2,
    });
    await store.setSyncInterval(30);
    expect(mockedApi.saveSettings).toHaveBeenCalledWith(["~/dev"], 3, 30, 2);
    expect(store.settings.sync_interval_mins).toBe(30);
  });

  it("setConcurrency 全量提交并回写设置", async () => {
    const store = useAppStore();
    store.settings = { scan_roots: ["~/dev"], max_depth: 3, sync_interval_mins: 10, concurrency: 2 };
    mockedApi.saveSettings.mockResolvedValueOnce({
      scan_roots: ["~/dev"],
      max_depth: 3,
      sync_interval_mins: 10,
      concurrency: 4,
    });
    await store.setConcurrency(4);
    expect(mockedApi.saveSettings).toHaveBeenCalledWith(["~/dev"], 3, 10, 4);
    expect(store.settings.concurrency).toBe(4);
  });

  it("revealLogs 调用后端打开日志目录", async () => {
    const store = useAppStore();
    await store.revealLogs();
    expect(mockedApi.revealLogsDir).toHaveBeenCalledTimes(1);
  });
});

describe("stats", () => {
  it("启用仓库按状态计数，无记录视作待同步，禁用仓库不计", async () => {
    const store = useAppStore();
    store.repos = [
      repo("~/a", { enabled: true }),
      repo("~/b", { enabled: true }),
      repo("~/c", { enabled: true }),
      repo("~/d", { enabled: true }),
      repo("~/e"),
    ];
    store.syncStates = {
      "~/a": syncState({ status: "synced" }),
      "~/b": syncState({ status: "conflict" }),
      "~/c": syncState({ status: "error" }),
    };
    expect(store.stats).toEqual({ synced: 1, pending: 1, conflict: 1, error: 1 });
  });

  it("lastSyncedAt 取全部记录的最大值", () => {
    const store = useAppStore();
    expect(store.lastSyncedAt).toBeNull();
    store.syncStates = {
      "~/a": syncState({ status: "synced", last_synced: 100 }),
      "~/b": syncState({ status: "synced", last_synced: 300 }),
      "~/c": syncState({ status: "pending", last_synced: null }),
    };
    expect(store.lastSyncedAt).toBe(300);
  });

  it("kindStats 按 remote 归属计数且不含 none", () => {
    const store = useAppStore();
    store.repos = [
      repo("~/a", { kind: "github_only" }),
      repo("~/b", { kind: "gitee_only" }),
      repo("~/c", { kind: "both" }),
      repo("~/d", { kind: "none" }),
    ];
    expect(store.kindStats).toEqual({ github: 1, gitee: 1, both: 1 });
  });
});

describe("resolveConflict", () => {
  it("成功时写入裁决后的状态", async () => {
    const store = useAppStore();
    store.syncStates["~/a/x"] = syncState({
      status: "conflict",
      conflicts: [
        { kind: "branch", name: "main", github_sha: "aaaaaaa", gitee_sha: "bbbbbbb" },
      ],
    });
    mockedApi.resolveConflict.mockResolvedValueOnce(
      syncState({ status: "synced", last_synced: 333 }),
    );
    await store.resolveConflict("~/a/x", "github");
    expect(mockedApi.resolveConflict).toHaveBeenCalledWith("~/a/x", "github");
    expect(store.syncStates["~/a/x"].status).toBe("synced");
  });

  it("失败时恢复原冲突状态并抛出错误", async () => {
    const store = useAppStore();
    const conflicted = syncState({
      status: "conflict",
      conflicts: [
        { kind: "tag", name: "v1.0", github_sha: "aaaaaaa", gitee_sha: "bbbbbbb" },
      ],
    });
    store.syncStates["~/a/x"] = conflicted;
    mockedApi.resolveConflict.mockRejectedValueOnce("强制推送被拒：远端有保护规则");
    await expect(store.resolveConflict("~/a/x", "gitee")).rejects.toBe(
      "强制推送被拒：远端有保护规则",
    );
    expect(store.syncStates["~/a/x"].status).toBe("conflict");
    expect(store.syncStates["~/a/x"].conflicts).toHaveLength(1);
    expect(store.conflictCount).toBe(1);
  });
});
