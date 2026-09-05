# GitFerry

简体中文 | [English](./README.en.md)

GitFerry 是一款 macOS 菜单栏常驻桌面应用：自动扫描本机的 git 仓库，按你为每个仓库配置的方向（GitHub ⇄ Gitee）同步代码——无需 webhook、无需服务器、复用系统 git 凭据。

## 特点

- **仓库自动发现**：按设定的扫描根目录和深度，自动找出现有 git 仓库，无需逐个添加。
- **双向同步**：每个仓库可独立配置方向——GitHub → Gitee、Gitee → GitHub，或双向。
- **一键同步 / 全部同步**：随时手动触发，仓库状态徽标实时反映同步结果（已同步 / 冲突 / 失败）。
- **冲突显式处理**：两端各有新提交时标记为冲突，由你三选一——以 GitHub 为准 / 以 Gitee 为准 / 手动合并，绝不自动选边。
- **令牌零落盘**：GitHub / Gitee 的 PAT 只存 macOS 钥匙串（服务名 `gitferry`），`config.json` 中不含任何令牌。
- **复用系统 git 凭据**：同步通过系统 `git` CLI 完成，凭证由钥匙串（osxkeychain）自理，软件不代管密码。
- **安全推送边界**：只推显式 refspec（`refs/heads/<branch>`、`refs/tags/<tag>`），绝不 `--mirror` / `--all`；自动路径仅 fast-forward，不自动强推、不自动删 ref。
- **本地日志可查**：滚动文件日志，可在设置中一键打开日志目录。

## 使用方法

### 1. 安装

从源码构建（见下文「开发」），或使用 `pnpm tauri build` 产出的 `.dmg` 安装。

### 2. 首次配置（向导）

1. 设置扫描根目录（如 `~/code`）与扫描深度；
2. 连接 GitHub / Gitee 账号：粘贴 PAT（Personal Access Token），应用会调用平台 API 验证后存入钥匙串；
3. 完成向导，进入主界面。

> 没有配置 PAT 也能同步：只要你在终端对该仓库 `git push` 过一次，系统钥匙串已有凭据即可。

### 3. 选择仓库并配置方向

- 主界面展示扫描到的仓库列表及各仓库远程归属（GitHub / Gitee / 两者）；
- 勾选启用需要同步的仓库，并为其选择同步方向；
- 点击「立即同步」同步单个仓库，或「全部同步」。

### 4. 处理冲突

当同一分支在两端各有新提交时，仓库会被标记为**冲突**并停止自动对齐。点击冲突仓库，按提示三选一：

- 以 GitHub 为准（把 Gitee 重置到 GitHub 状态）；
- 以 Gitee 为准（把 GitHub 重置到 Gitee 状态）；
- 手动合并（自己到终端处理，之后继续同步）。

### 平台限制提示

- Gitee：单文件 > 100 MB 推送会失败；免费仓库容量约 1 GB；不支持 Git-LFS 对象。
- 仓库路径失效（被移动/删除）会标记为 `missing`，配置保留，不会自动删除。

## 开发

```bash
pnpm install          # 安装前端依赖
pnpm tauri dev        # 开发运行（vite + cargo）
cargo test            # Rust 单测（src-tauri 下执行）
pnpm build            # 前端类型检查（vue-tsc）+ 构建
pnpm tauri build      # 打包 .dmg
```

### 技术栈

- **后端**：Rust + Tauri 2（`std::process::Command` 调 `git` CLI，keyring 存取 PAT，reqwest 验证账号）
- **前端**：Vue 3 + TypeScript + Pinia + Tailwind CSS + shadcn-vue
- **构建**：Vite 6 + pnpm

### 路线图

- [x] M1：扫描器、仓库识别、账号连接、配置持久化
- [x] M2：同步引擎（fetch/push、tag 补推、分叉检测、冲突解决）
- [ ] M3：轮询调度（定时自动同步）、开机自启

## 设计文档

同步算法与安全边界的唯一权威来源：[design/github-gitee-sync-design.md](./design/github-gitee-sync-design.md)

## License

MIT
