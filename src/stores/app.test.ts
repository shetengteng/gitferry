import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppStore } from "./app";
import type { RepoEntry } from "@/lib/types";

vi.mock("@/lib/invoke", () => ({
  getAppState: vi.fn(),
  scanRepos: vi.fn(),
  setRepoConfig: vi.fn().mockResolvedValue(undefined),
  setReposConfig: vi.fn().mockResolvedValue(undefined),
  saveSettings: vi.fn(),
  configureAccount: vi.fn(),
  completeSetup: vi.fn().mockResolvedValue(undefined),
}));

import * as api from "@/lib/invoke";

function repo(path: string, overrides: Partial<RepoEntry> = {}): RepoEntry {
  return {
    path,
    enabled: false,
    direction: "both",
    remotes: [],
    kind: "both",
    ...overrides,
  };
}

const mockedApi = vi.mocked(api);

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
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
