# 安全红线（全项目生效，不可协商）

以下规则源自 `docs/github-gitee-sync-design.md` 的边界策略，任何代码改动不得违反。

## 令牌与凭据

1. PAT 令牌**只存 macOS 钥匙串**（`keyring` crate，服务名 `gitferry`，账户名 `github`/`gitee`）；`config.json` 只允许存平台用户名与连接状态。
2. **令牌绝不落盘**：不得写入 config.json、日志、调试输出、错误信息、前端 state 持久化。
3. 软件不代管 git 密码：凭证问题引导用户去终端 `git push` 一次触发钥匙串存凭据，或在账号页配置 PAT。

## git 数据安全

4. 任何可能丢提交的操作（`--force` push、删除 ref）**必须用户显式确认**；自动同步路径只允许 fast-forward。
5. 分支分叉（两端各有新提交）必须停下标记 `conflict` 并告警，由用户三选一（以 GitHub 为准 / 以 Gitee 为准 / 手动合并），禁止自动选择权威侧。
6. tag 已存在但指向不同 → 视为冲突不覆盖，禁止强推 tag。
7. 默认不同步分支/tag 的删除；仅「镜像模式」开启后按 refspec 对齐，且属用户显式选择。

## 推送范围

8. 只推显式 refspec（`refs/heads/<branch>`、`refs/tags/<tag>`），绝不 `--mirror` / `--all`。
9. 扫描与同步操作不得修改用户仓库的工作区（不 checkout、不 rebase、不 reset）；本地落后时仅允许 `git pull --ff-only`。

## 平台限制（写 UI 文案/判定时必须考虑）

10. Gitee：单文件 >100MB 推送失败；免费仓库约 1GB；不支持推送 Git-LFS 对象（v1 不处理 LFS）。
11. 仓库路径失效（被移动/删除）标记 `missing`，保留配置，不得自动删除用户配置。

## 失败与退避

12. 仓库连续失败 3 次后退避到 3× 轮询间隔，避免对无凭证仓库反复触发钥匙串弹窗。
13. 同步期间用户正在操作仓库导致 push/fetch 失败：重试 1 次，仍失败本轮跳过，不得阻塞队列。
