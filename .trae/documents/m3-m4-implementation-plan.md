# GitFerry M3 常驻自动化 + M4 镜像模式 实施计划

## Context

M1（扫描/列表/账号/设置）与 M2（同步引擎 + 冲突裁决）已完成并带单测。剩余未完成：

- **M3**：`scheduler.rs` 轮询调度（`sync_interval_mins`/`concurrency` 配置已有但无消费方）、托盘常驻、开机自启、单实例锁、关窗隐藏到托盘、令牌低频校验
- **M4**：镜像模式 `mirror_delete`、打包 `.dmg`
- 最终目标：全部完成后进行全量测试（cargo test + pnpm test + pnpm build + 手动 e2e + 打包）

设计权威：`design/github-gitee-sync-design.md` §4.4 调度、§5 安全边界、§7 常驻自启。

## 实施步骤（按依赖顺序）

### A. 依赖与资源

- `src-tauri/Cargo.toml`：tauri features 追加 `"tray-icon"`, `"image-png"`；新增 `tauri-plugin-autostart = "2"`、`tauri-plugin-single-instance = "2"`。不引入 tokio 直接依赖
- `package.json`：`pnpm add @tauri-apps/plugin-autostart`
- `src-tauri/capabilities/default.json`：permissions 追加 `"autostart:default"`（托盘/single-instance 纯 Rust 侧无需权限）
- 生成 4 个托盘状态圆点 PNG（32×32）到 `src-tauri/icons/tray/`：idle 灰 `#8E8E93` / syncing 蓝 `#0A84FF` / conflict 黄 `#FFCC00` / error 红 `#FF3B30`（临时 Python 脚本生成后删除脚本，产物入库）

### B. 数据契约（sync.rs + config.rs + types.ts 三方同步）

- `config.rs` `RepoEntry` 加 `#[serde(default)] pub mirror_delete: bool`；补单测：旧 JSON（无该字段）解析为 false
- `sync.rs` `RepoSyncState` 加 `#[serde(default)] pub deleted_refs: u32`；修补 `finish()` 与 `commands.rs::mark_syncing` 两处全字段字面量构造
- `src/lib/types.ts`：`RepoEntry.mirror_delete: boolean`、`RepoSyncState.deleted_refs: number`

### C. sync.rs 镜像删除（M4 核心）

- `Acc` 加 `deleted: u32`
- `round_refs(root, round, remotes, acc, mirror_delete: bool)`：分支循环后、tag 循环后各加反向 diff——遍历 dst 侧 refs，src 确认不存在且 mirror_delete → 删除
- 新增 `push_delete(root, remote, target, kind, name, acc)`：`git push <remote> --delete <显式 refspec>`（`refs/heads/<b>` / `refs/tags/<t>`，绝不 `--mirror` 裸推）；成功 `acc.deleted += 1`，失败走 `classify_git_error` 记入 errors
- `finish()` 写入 `deleted_refs`，log 加字段
- 语义：沿同步方向单向清理（`both` 两轮对称收敛；`github_to_gitee` 只清理 Gitee 侧）
- 红线合规五点：仅删「src 确认不存在」的 ref；ref 列表获取失败直接 return 不删；开关关闭立即停删；config 默认 false；UI 开启时显式确认

### D. auth.rs

- 新增 `pub fn get_token(platform: Platform) -> FerryResult<Option<String>>`（keyring `NoEntry` → `Ok(None)`；令牌瞬时存在，不缓存不落日志）。供调度器低频校验复用已有 `auth::verify`

### E. commands.rs（事件通道 + 新 command + sync_all 并发化）

- `mark_syncing` / `store_state` / `lock_sync` 改 `pub(crate)`（scheduler 复用）
- 新增 `pub(crate) fn emit_sync_states(app: &AppHandle)`：收集 SharedSync 全量 → `app.emit("sync-states-changed", Vec<RepoSyncResult>)` → 调 `tray::update(app)`（托盘刷新唯一入口）
- `sync_now` / `sync_all` / `resolve_conflict` 加 `app: tauri::AppHandle` 参数，写回状态后 emit（返回值契约不变）
- `sync_all` 执行段改调 `scheduler::execute_entries`（并发限流），保留「返回全部仓库、未启用保持既有状态」组装
- 新增 command `set_repo_mirror(state, path, mirror_delete)`：mutate 模式改字段；`lib.rs` generate_handler 注册（三处对齐：command 定义 / generate_handler / invoke.ts）

### F. scheduler.rs（新模块）

线程模型：**std::thread 常驻线程**（工作是阻塞 git 子进程；sleep 用 std；唯一 async 点令牌校验用 `tauri::async_runtime::block_on`）。并发限流用**分批 spawn+join**（chunks(concurrency)，零新依赖）。

```
pub const FIRST_ROUND_DELAY_SECS: u64 = 30;   // 启动延迟首轮
pub const RETRY_TIMES: u32 = 1;               // error 单轮内重试 1 次（safety 13）
pub const BACKOFF_THRESHOLD: u32 = 3;         // 连败 3 次退避（safety 12）
pub const BACKOFF_MULTIPLIER: u32 = 3;
pub const TOKEN_VERIFY_INTERVAL_MINS: u32 = 360;  // 令牌 6h 低频校验

struct RepoSchedule { fail_streak: u32, last_attempt: Option<i64> }  // 内存不落盘

pub struct Scheduler {  // manage 注入
    schedules: Mutex<HashMap<PathBuf, RepoSchedule>>,
    round_busy: AtomicBool,             // 调度轮与托盘手动触发互斥
    last_token_verify: Mutex<Option<Instant>>,
}

pub fn spawn(app: AppHandle);                       // setup 中启动常驻线程
pub fn trigger_manual_round(app: &AppHandle);       // 托盘入口：try 占用，占用中忽略并 log
pub(crate) fn execute_entries(app, entries, concurrency) -> Vec<(PathBuf, RepoSyncState)>;
fn sync_with_retry(entry) -> RepoSyncState;         // 仅 Error 重试 1 次；conflict/missing 不重试
fn run_round(app, respect_backoff: bool);           // 选仓库→执行→更新 schedules→emit
// 纯函数（单测对象）：
fn backoff_multiplier(fail_streak: u32) -> u32;
fn is_due(s, interval_mins, now) -> bool;           // now >= last + interval*60 × 退避倍数
fn select_repos(entries, states, schedules, now, respect_backoff) -> Vec<RepoEntry>;
// 过滤 enabled && kind!=None；跳过 status ∈ {Conflict, Missing, Syncing}；respect_backoff 时按 is_due
fn maybe_verify_tokens(app);  // 每 360min：connected 账号 verify，Unauthorized → 置 invalid 持久化
```

关键点：每轮开头重读 config（间隔变更下轮生效，与现有设置页文案一致）；config 锁只做快照 clone 绝不跨 git 子进程持有；join Err 降级为 Error 态不让单仓 panic 杀死循环。

### G. tray.rs（新模块）+ lib.rs 总装

- `aggregate(map) -> TrayStatus{Idle,Syncing,Conflict,Error}`：Syncing > Conflict > Error(含 Missing) > Idle；`status_line` / `tooltip_for` 纯函数可单测（「就绪 / 同步中… / N 个冲突待处理 / N 个出错」）
- 菜单：状态计数行（禁用，set_text 更新）/ 打开主窗口 / 立即全量同步 / 退出；`show_menu_on_left_click(true)`
- `TrayIconBuilder::with_id("gitferry-tray")` + `include_image!` 加载图标（不用 `icon_as_template`，保彩色）；事件：show→show+set_focus，sync_all→`scheduler::trigger_manual_round`，quit→`app.exit(0)`
- lib.rs：
  - `.plugin(tauri_plugin_single_instance::init(...))` **必须第一个注册**（回调 show+unminimize+set_focus 主窗口）
  - `.plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))`
  - `.manage(Scheduler::new())`
  - `.on_window_event`：CloseRequested → `prevent_close()` + `hide()`（关窗隐藏到托盘）
  - `.setup`：tray::init → scheduler::spawn(handle.clone())
  - `.run` 处理 `RunEvent::Reopen`（macOS Dock 点击）→ show+set_focus 主窗口

### H. 前端

- `lib/invoke.ts`：`setRepoMirror(path, mirrorDelete)` → `invoke("set_repo_mirror", { path, mirrorDelete })`
- `lib/events.ts`（新建）：`listenSyncStates(cb)` 封装 `listen<RepoSyncResult[]>("sync-states-changed", ...)`
- `lib/dialog.ts`：`askConfirm(title, message)` 封装 plugin-dialog `ask(..., { kind: "warning" })`
- `stores/app.ts`：
  - `init()` **先 await bindSyncEvents() 再并行请求**（消除事件与初始拉取的竞态）；模块级 unlisten 防重复订阅
  - 事件回调 **merge** 语义：`syncStates = { ...old, ...resultsToStates(payload) }`
  - 新 action `setRepoMirror(path, mirrorDelete)`
- `RepoRow.vue`：第二行方向 Select 旁加「镜像删除」Switch（`v-if enabled && kind!=='none'`，`:model-value` 单向绑定天然回弹）；开启时 `askConfirm`（中文警告：沿同步方向删除远端多余分支/标签、不可自动撤销），关闭直接 emit；emits 加 `mirror`
- `MainView.vue`：`@mirror` → `store.setRepoMirror`
- `SettingsView.vue`：「并发同步数」后加「开机自启」行：onMounted `isEnabled()` 初始化，切换 `enable()/disable()`，失败就地小字错误（参照 revealError 模式）；加灰色说明「正式版安装后请重新开关一次以更新启动路径」（dev 模式注册的是 debug 路径）
- `app.test.ts`：新增 setRepoMirror（成功更新/失败不变）、bindSyncEvents（merge、防重复订阅）用例；mock 清单同步扩充

## 测试与验证

### Rust 单测（cargo test 全绿）

- scheduler：`backoff_multiplier` 阈值边界；`is_due` 普通/退避间隔；`select_repos` 过滤 disabled/none/conflict/missing/退避中，手动触发绕过退避；tray `status_line`/`tooltip_for` 四态
- sync 镜像删除 4 用例（复用 TestEnv/bare_sha/push_foreign_commit；删 bare ref 用 `update-ref -d`）：
  1. Both+mirror：github 删 feature → gitee 消失，deleted_refs≥1，status=Synced
  2. Both+mirror：github 删 tag v1 → gitee 消失
  3. mirror=false：远端删分支后对端保留、deleted_refs=0（默认行为回归保护）
  4. Both+mirror 对称：github 删 a、gitee 删 b → 两端均收敛、deleted_refs=2，无 conflicts
- config：mirror_delete serde default false + roundtrip true

### 前端（pnpm test + pnpm build）

vitest 新用例见上；`pnpm build`（vue-tsc）保证类型契约。

### 手动 e2e 清单（pnpm tauri dev）

1. 关窗不退出，托盘「打开主窗口」恢复；Dock 点击 Reopen 恢复
2. 间隔调 1min 观察自动轮询；UI 状态徽标随事件实时刷新（无手动刷新）
3. 托盘「立即全量同步」：图标 蓝→灰、UI 更新；轮询中手动触发被忽略（log）
4. 失败仓库连败 3 次 → log 显示 3× 退避
5. 四态托盘：conflict→黄、error→红、tooltip/计数文案
6. 镜像删除：远端删分支 → 下轮对端消失；关开关停删
7. 自启：Switch 开 → 系统设置登录项出现；重启自启
8. Finder 双开 → 单实例、已有窗口聚焦
9. 令牌失效 → 账号徽标「令牌失效」且 config.json 持久化
10. `pnpm tauri build` 出 .dmg，安装后回归 1/7/8

## 风险与规避

- 托盘不用 `icon_as_template`（否则彩色圆点全部变单色）
- single-instance 必须第一个注册；回调防御 `if let Some(w)`
- 镜像删除 code review 对照红线五点逐条核验
- 钥匙串弹窗防护三层：GIT_TERMINAL_PROMPT=0（已有）+ 连败退避 + 单轮重试 1 次
- dev 模式 autostart 注册 debug 路径，打包后需重新开关（设置页文案说明）
