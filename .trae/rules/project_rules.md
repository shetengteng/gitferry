# GitFerry 项目规则

## 项目定位

GitFerry 是一个 macOS 菜单栏常驻桌面应用：自动扫描本机所有 git 仓库，按仓库配置的同步方向（GitHub ⇄ Gitee）后台定时自动同步。无需 webhook、无需服务器、复用系统 git 凭据。

## 技术栈

- **后端**：Rust + Tauri 2（`src-tauri/`）
- **前端**：Vue 3 + TypeScript + Pinia + Tailwind CSS + shadcn-vue（reka-ui）
- **构建**：Vite 6 + pnpm；Rust 侧 cargo
- **同步引擎**：调 `git` CLI（`std::process::Command`），**不用 git2 crate**——凭证由系统钥匙串自理

## 目录结构

```
src/                      # Vue 前端
  components/             # 业务组件（MainView、RepoRow、Wizard 等）
  components/ui/          # shadcn-vue 生成的基础组件（button/dialog 等）
  stores/                 # Pinia store（app.ts）
  lib/                    # invoke.ts（Tauri 调用封装）、types.ts（类型）、utils.ts
src-tauri/src/
  commands.rs             # #[tauri::command] 入口，经 lib.rs generate_handler! 注册
  scanner.rs              # 目录遍历发现 .git（walkdir）
  repo.rs                 # 解析 .git/config remotes，host 归属判定
  auth.rs                 # 平台账号：PAT 存取（keyring）+ API 验证
  config.rs               # 配置读写（~/Library/Application Support/gitferry/config.json）
  error.rs                # FerryError / FerryResult
  logging.rs              # tracing 滚动文件日志
design/github-gitee-sync-design.md   # 设计文档（架构与同步算法的唯一权威来源）
```

待开发模块（见设计文档 §4、§10 里程碑）：`sync.rs`（同步引擎，M2）、`scheduler.rs`（轮询调度，M3）、`autostart.rs`（自启，M3）。

## 架构边界

- **前端只做展示与交互**：所有系统能力（文件扫描、git 操作、keyring、配置持久化）必须通过 Tauri command 走 Rust 侧，前端不直接访问文件系统/网络。
- **Rust 侧不碰 UI**：command 返回序列化数据，不返回 HTML/样式；用户可见的文案错误用中文给出可操作提示。
- **数据契约单一**：前后端共享的数据结构以 `src-tauri/src/config.rs` 的 serde 定义为准，前端 `src/lib/types.ts` 手工镜像对齐，字段名 snake_case。
- **设计文档优先**：涉及同步算法、冲突策略、调度行为时，先读 `design/github-gitee-sync-design.md`，不得偏离其中约定的安全边界。

## 开发命令

```bash
pnpm install            # 安装前端依赖
pnpm tauri dev          # 开发运行（自动起 vite + cargo）
cargo test              # Rust 单测（在 src-tauri/ 下执行）
pnpm build              # 前端类型检查（vue-tsc --noEmit）+ 构建
pnpm tauri build        # 打包 .dmg
```

## 通用约定

- 所有面向用户的文案（错误提示、状态、按钮）使用**中文**。
- Rust 侧日志统一用 `tracing`（`tracing::info!` 等），禁止 `println!`/`dbg!` 进入库代码。
- 提交前：Rust 改动跑 `cargo test`；前端改动跑 `pnpm build`（含 vue-tsc 类型检查）。
- 新增 Tauri command 必须同时在 `commands.rs` 定义、`lib.rs` 的 `generate_handler!` 注册、`src/lib/invoke.ts` 封装，三处缺一不可。
