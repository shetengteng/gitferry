# GitFerry 实施进度

> 对照 [github-gitee-sync-design.md](./github-gitee-sync-design.md) 的架构（§4）与里程碑（§10）整理。
> 更新日期：2026-09-05

## M2.5 UI 对齐原型 —— ✅ 完成

前端 UI 已按 [gitferry-prototype.html](./gitferry-prototype.html) v2 对齐：主界面两行页头 + 统计卡行 + 卡片化仓库行（冲突左条纹）、设置页新增「自动同步间隔 / 并发同步数」（持久化，M3 调度器消费）与「打开日志目录」（`reveal_logs_dir` command）、向导支持目录增删与 remote 统计、rail 加 logo。设置页「登录自启 / 关闭保留托盘」两项随 M3 托盘/autostart 一并实现，暂未展示。

## M1 扫描与列表 —— ✅ 基本完成

| 模块/功能 | 状态 | 说明 |
|---|---|---|
| `scanner.rs` 目录扫描 | ✅ | walkdir 遍历、深度限制、排除 `node_modules`/隐藏目录，带单测 |
| `repo.rs` 仓库识别 | ✅ | 解析 remotes、host 归一化（github/gitee/other）、kind 分类，带单测 |
| `auth.rs` 账号配置与检测 | ✅ | PAT 存取 keyring + GitHub/Gitee API 验证，带单测 |
| `config.rs` 配置持久化 | ✅ | `~/Library/Application Support/gitferry/config.json`，带单测 |
| `logging.rs` 日志 | ✅ | tracing 滚动文件（原计划属 M3，已提前完成） |
| `error.rs` 错误体系 | ✅ | FerryError/FerryResult，中文文案序列化透出前端 |
| 仓库列表 UI | ✅ | MainView / RepoRow / StatusBadge，含搜索过滤 |
| 首次启动向导 | ✅ | Wizard：选目录 → 扫描 → 批量配置方向 → 完成 |
| 设置页 | ⚠️ 部分完成 | 目前仅扫描目录 + 深度；轮询间隔/并发数/开机自启属 M3 范围未做 |
| 账号对话框 | ✅ | AccountDialog |
| 前端状态管理 | ✅ | Pinia app store + `app.test.ts` 单测 |
| invoke 封装 | ✅ | 7 个 command 三处齐备（commands.rs / lib.rs / invoke.ts），含批量配置 `set_repos_config` |

## M2 同步引擎 —— ✅ 基本完成

| 模块/功能 | 状态 | 说明 |
|---|---|---|
| `sync.rs` 同步引擎 | ✅ | fetch 两端 → 逐分支比较（缺分支补建 / fast-forward / 祖先跳过 / 分叉记 conflict）→ tag 补推与漂移拒推；本地 checkout 分支三方比较（领先 push、落后 `pull --ff-only`、分叉 conflict）；只推显式 refspec，自动路径无 `--force`；11 个单测（tempfile + bare 仓库） |
| `sync_now` / `sync_all` command | ✅ | `spawn_blocking` 执行，运行时状态 `SharedSync`（内存，不落盘）；sync_all 顺序执行（并发限流留给 M3） |
| `get_sync_states` | ✅ | 前端 init 时拉取合并 |
| 冲突裁决 `resolve_conflict` | ✅ | 用户三选一后按权威侧 force 对齐（全项目唯一 `--force` 路径），随后复合同步 |
| 前端同步状态 | ✅ | types/invoke/store 对齐契约；RepoRow 状态徽标（已同步/待同步/同步中/冲突/错误/路径失效）+ 相对时间 + 立即同步按钮；MainView 立即全量同步 + 状态栏摘要 |
| 冲突告警 UI | ✅ | ConflictDialog：分支/tag 冲突列表（双侧短 sha）+ 三选一（以 GitHub 为准 / 以 Gitee 为准 / 手动合并），含覆盖不可恢复警示 |
| 单测 | ✅ | cargo test 24 全绿（sync 11 个）；前端 vitest 14 全绿 |

## M3 常驻自动化 —— ❌ 未开始

- [ ] `scheduler.rs` 轮询调度（默认 10min 间隔 + 队列并发限流，默认并发 2）
- [ ] 失败退避（连续失败 3 次 → 3× 间隔）
- [ ] 托盘常驻（tray 图标状态：空闲/同步中/冲突/错误 + 菜单项）
- [ ] `autostart.rs` 开机自启（tauri-plugin-autostart，依赖尚未引入）
- [ ] 单实例锁（tauri-plugin-single-instance，依赖尚未引入）
- [x] 日志（logging.rs 已提前完成）

## M4 打磨与打包 —— ❌ 未开始

- [ ] 镜像模式（同步分支/tag 删除）
- [ ] 强制覆盖选项（用户显式选定权威侧）
- [ ] 端到端打磨（真实 GitHub/Gitee 测试仓库验证三方向、冲突告警、凭证失败提示）
- [ ] 打包 `.dmg`

## v2 备选（暂不实施）

- [ ] Spotlight 加速扫描（`mdfind -name .git` 预筛）
- [ ] 自动建仓（gitee-only/github-only 一键创建目标仓库，§6.1）
- [ ] Git-LFS 对象同步

## 总结

M1、M2 完成。M2 实现要点：同步状态为运行时内存态（重启后按 pending 处理）；tag 比较用 `git ls-remote --tags` 读两端真实 refs（peeled sha 比指向、ref sha 补推）；`GIT_TERMINAL_PROMPT=0` 避免无凭据仓库挂起；fetch 失败本轮即 error，不基于过期数据判定祖先。下一步 M3：调度轮询、托盘常驻、自启、并发限流与退避。
