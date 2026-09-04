# GitFerry 设计：GitHub ⇄ Gitee 本地自动同步软件

> 形态：macOS 菜单栏常驻桌面应用。自动扫描本机 git 仓库，按仓库配置开关与同步方向，后台定时自动同步，无需 webhook、无需服务器。
> 技术栈沿用 workspace 约定：Rust + Tauri 2 + Vue 3 + Pinia。独立仓库，代号 `gitferry`。

## 1. 目标

- **自动发现**：扫描 Mac 上指定目录下的所有 git 仓库，识别 remote 归属（GitHub / Gitee / 双端 / 其他）。
- **按仓库开关**：每个仓库独立启用/停用，方向可选 `GitHub → Gitee`、`Gitee → GitHub`、`双向`。
- **自动工作**：登录自启、菜单栏常驻，按间隔轮询自动检查更新并推送，全程免人工。
- **零额外配置**：复用系统 `git` 与钥匙串凭据（用户平时能 push 就能同步），不要求配 webhook/Action/token。

## 2. 核心调研结论（git 层，方案无关，仍有效）

1. 同步本质是 ref 级操作：比较两端同名 ref，领先侧 push 到落后侧。**不需要 mirror clone**，直接用本地仓库做中转。
2. `git push --mirror` 会连带 `refs/pull/*`（GitHub 特有）污染 Gitee，必须用显式 refspec 只推 `refs/heads/*`、`refs/tags/*`。
3. 认证走 git 自身凭据链（macOS `osxkeychain`）：本机能 `git push` 的仓库，同步引擎调 CLI 即可直接推，软件不碰明文密码。
4. Gitee 限制：单文件 >100MB 推送失败；免费仓库约 1GB；不支持推送 Git-LFS 对象（`lfs` 内容需单独处理，v1 不做）。
5. 两端各有新提交（分叉）时，任何自动 force 都可能丢数据——**分叉必须停下告警**，由用户决定。

## 3. 产品形态与交互

- **菜单栏常驻**（`tray`）：图标显示全局状态（空闲/同步中/有冲突/错误），菜单项：打开主窗口、立即全量同步、退出。
- **主窗口**：仓库列表，每行展示：
  - 仓库路径与名称
  - remote 归属徽标（`github` / `gitee` / `both`）
  - 同步方向选择器（关 / →Gitee / →GitHub / 双向）
  - 最近同步时间、状态（`synced` / `ahead` / `conflict` / `error`）
  - 展开详情：分支差异列表、冲突分支、错误信息、操作按钮（立即同步、查看日志）
- **设置页**：扫描目录列表、扫描深度、轮询间隔、开机自启、并发数。
- **首次启动向导**：选扫描目录 → 扫描 → 批量勾选启用方向 → 完成。

## 4. 架构

```
src/                      # Vue 前端（列表、设置、向导）
  stores/                 # Pinia：repos、settings、sync 状态
  lib/                    # invoke 封装
src-tauri/src/
  commands.rs             # #[tauri::command]：scan、list、set_repo_config、sync_now、get/set_settings
  scanner.rs              # 目录遍历发现 .git
  repo.rs                 # 读 .git/config 解析 remotes；归一化 URL 归属判定
  sync.rs                 # 同步引擎：fetch、ref 比较、push、冲突检测
  scheduler.rs            # 轮询定时器 + 队列（并发限流）
  config.rs               # 配置读写（~/Library/Application Support/gitferry/config.json）
  logging.rs              # tracing 滚动文件（~/Library/Logs/gitferry/app.log）
  autostart.rs            # tauri-plugin-autostart 封装
```

- **同步引擎调 `git` CLI**（`std::process::Command`），不用 `git2`：凭证由 git 凭证链自理，submodule/worktree/稀疏检出等边缘行为与本机手工操作完全一致。
- 每仓库一个同步任务；全局并发默认 `2`，避免同时 fetch 大仓库拖满磁盘与网络。

### 4.1 扫描（`scanner.rs`）

- 默认目录：`~/Documents`、`~/Downloads`（浅层）、`~/Projects`、`~/dev`、`~/code` 中存在的项，用户可增删。
- 实现：`walkdir` 递归找名为 `.git` 的目录/文件（worktree 的 `.git` 是文件），默认深度 `4`，跳过 `node_modules`、`vendor`、`.Trash`、隐藏目录（扫描根本身除外）。
- 结果缓存到 `config.json`；仅菜单栏「重新扫描」或目录变更时全量扫描，轮询只对已登记仓库工作。
- 可选加速（macOS）：`mdfind -name .git`（Spotlight）预筛，v2 再做。

### 4.2 仓库识别（`repo.rs`）

- 解析 `.git/config` 中 `[remote "<name>"] url`，按 host 归一化：
  - `github.com` → `github`；`gitee.com` → `gitee`；其余 → `other`（仅展示，不可同步）。
- 分类：`github-only` / `gitee-only` / `both` / `none`。
- `gitee-only` 仓库在启用「→GitHub」时提示需要目标仓库存在；见 §6 自动建仓。

### 4.3 同步引擎（`sync.rs`）

对单个已启用仓库：

```
1. git fetch <src-remote> --prune            # 更新 remote-tracking refs
2. 对 refs/heads/* 逐分支比较 src-sha 与 dst-sha（dst 侧先 git fetch）
   - dst 缺分支        → push 领先侧：git push <dst> <sha>:refs/heads/<branch>
   - src 是 dst 祖先    → 无操作
   - dst 是 src 祖先    → fast-forward push
   - 分叉               → 标记 conflict，跳过该分支，UI 告警
3. refs/tags/*：dst 缺失的 tag 逐个补推；已存在的 tag 不覆盖（tag 漂移视为冲突，告警）
4. 删除同步：默认不同步分支/tag 删除；「镜像模式」开关打开后按 refspec 强推对齐
```

- 只推显式 refspec（`refs/heads/<branch>`、`refs/tags/<tag>`），绝不 `--mirror` / `--all` 裸推。
- push 一律不带 `--force`，除非用户对该仓库显式开启「强制覆盖」并选定权威侧。
- 输出解析：git stderr 中的 `Everything up-to-date` → `synced`；`[rejected]`（non-fast-forward）→ `conflict`；认证/网络失败 → `error` + 原始错误透出到 UI。
- 凭证提示：若 push 因凭证失败，引导用户到终端 `git push` 一次以触发钥匙串存凭据（或配置 PAT），软件不代管密码。

### 4.4 调度（`scheduler.rs`）

- 默认间隔 `10min`（可配 `5min`–`6h`）；菜单栏「立即全量同步」即时触发。
- 对每个启用仓库：先 `git fetch` 两端比较远端 refs，**远端已一致则跳过 push**（即"自动检查更新"），有差异才推。
- 指数退避：仓库连续失败 3 次后退避到 3× 间隔，避免对无凭证仓库反复打钥匙串弹窗。

## 5. 冲突与安全（边界策略）

| 情形 | 行为 |
|---|---|
| 分支分叉（两端各有新提交） | 停止该分支，状态 `conflict`；UI 给三选一：以 GitHub 为准 / 以 Gitee 为准 / 本地手动合并。选定后执行一次带 `--force` 的单向对齐并记录日志 |
| tag 已存在但指向不同 | 视为冲突，不覆盖 |
| 同步期间用户正在操作仓库 | push/fetch 失败重试 1 次；仍失败则本轮跳过 |
| 本地仓库有未推送提交 | 本地提交所在分支按「本地 ↔ 两远端」三方比较：本地领先则先 push 到其 origin；本地落后则 `git pull --ff-only`。拉取失败（分叉）→ `conflict` |
| 仓库路径失效（被移动/删除） | 标记 `missing`，不出现在轮询中，保留配置 |

原则：**任何可能丢提交的操作（force、删除 ref）必须用户显式确认**；自动路径只做 fast-forward。

## 6. 自动建仓（v2，可选）

`gitee-only`/`github-only` 仓库开启另一侧方向时，目标仓库可能不存在：

- Gitee：`POST https://gitee.com/api/v5/user/repos`（私人令牌，`projects` 权限）
- GitHub：`POST https://api.github.com/user/repos`（PAT，`repo` 权限）

令牌经 `keyring` crate 存入 macOS 钥匙串；UI 上是启用开关旁的「目标仓库不存在，一键创建」提示。v1 仅提示用户手动建仓。

## 7. 常驻与自启

- `tauri-plugin-autostart` 实现登录自启；关闭主窗口仅隐藏到托盘，托盘「退出」才真正结束进程。
- 单实例锁（`tauri-plugin-single-instance`），避免重复扫描与双份轮询。

## 8. 配置与日志

- `config.json`（`~/Library/Application Support/gitferry/`）：扫描根、深度、间隔、并发、每仓库 `{path, enabled, direction, mirror_delete, force_side}`。
- 日志：`~/Library/Logs/gitferry/app.log` 滚动文件，每仓库每次同步一条结果记录（分支数、推送 ref 数、耗时、错误）。
- 所有用户可见错误给可操作提示（如「gitee push 被拒：需要先完成一次终端 push 以保存凭据」）。

## 9. 测试计划

Rust 单测（`cargo test`）：

| 模块 | 用例 |
|---|---|
| `scanner` | 深度限制、`.git` 文件（worktree）、排除 `node_modules`、隐藏目录 |
| `repo` | URL 归一化（`git@github.com:owner/repo.git`、`https://gitee.com/owner/repo`、带端口自定义 host）、`both` 分类 |
| `sync` | 用本地 bare 仓库伪造两端：fast-forward 推送、tag 补推、分叉检测、tag 漂移拒推、镜像模式删 ref |
| `scheduler` | 失败退避、并发限流 |

端到端（手动清单）：真实 GitHub/Gitee 各建测试仓库 → 验证三方向、冲突告警、凭证失败提示、自启与托盘常驻。

## 10. 里程碑

1. **M1 扫描与列表**：`scanner` + `repo` + 仓库列表 UI + 配置持久化（无同步）。
2. **M2 同步引擎**：`sync.rs` 三方向 fast-forward 同步 + 手动触发 + 冲突检测。
3. **M3 常驻自动化**：调度轮询、托盘、自启、日志。
4. **M4 打磨**：镜像模式、退避、端到端打磨、打包 `.dmg`（`tauri-app` skill）。
