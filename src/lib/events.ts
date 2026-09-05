import { listen } from "@tauri-apps/api/event";
import type { RepoSyncResult } from "./types";

/** 订阅后端同步状态变更事件，返回解除订阅函数 */
export function listenSyncStates(
  cb: (results: RepoSyncResult[]) => void,
): Promise<() => void> {
  return listen<RepoSyncResult[]>("sync-states-changed", (e) => cb(e.payload));
}
