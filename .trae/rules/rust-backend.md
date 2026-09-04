---
paths:
  - "src-tauri/**"
  - "Cargo.toml"
---

# Rust / Tauri 2 后端规则

## 模块组织

- 模块划分遵循 `docs/github-gitee-sync-design.md` §4：`commands.rs`（command 入口）、`scanner.rs`、`repo.rs`、`sync.rs`、`auth.rs`、`scheduler.rs`、`config.rs`、`logging.rs`、`autostart.rs`。
- `commands.rs` 只做参数接收、状态读写与调度，具体逻辑下沉到领域模块（scanner/repo/sync 等），保持 command 层薄。
- 新增模块在 `lib.rs` 中 `mod` 声明；新增 command 必须在 `lib.rs` 的 `generate_handler!` 中注册。

## 错误处理（强制）

- 统一用 `crate::error::{FerryError, FerryResult}`，禁止在库代码中 `unwrap()`/`expect()`（锁中毒的 `expect("config poisoned")` 模式除外，沿用现状）。
- 新错误类别优先扩展 `FerryError` 枚举（thiserror），临文案用 `FerryError::msg(...)`。
- `FerryError` 序列化为字符串直接透出前端，因此**错误文案必须是用户可读的中文并附带可操作建议**。
- command 返回值一律 `FerryResult<T>` 或确定性数据，不 panic。

## 状态与配置

- 共享状态用 `commands::SharedConfig = Mutex<Config>`，经 `.manage()` 注入。
- 修改配置必须走 `commands::mutate()` 模式：闭包内改 `Config`，成功后自动 `config.save()` 持久化，禁止改内存不落盘。
- 配置路径固定 `~/Library/Application Support/gitferry/config.json`（`config.rs`）。

## serde 契约

- 所有跨端枚举/结构体：`#[derive(Serialize, Deserialize)]` + `#[serde(rename_all = "snake_case")]`（枚举）。
- 改动 `config.rs` 中的数据结构时，必须同步更新前端 `src/lib/types.ts`，保持字段与取值一致。

## git 操作（强制）

- 同步引擎通过 `std::process::Command` 调 `git` CLI，**禁止引入 `git2`/`gix` crate**。
- 凭证由 git 凭证链（osxkeychain）自理：代码中不得读取、缓存或代填用户密码。
- 只推显式 refspec（`refs/heads/<branch>`、`refs/tags/<tag>`），**禁止 `--mirror`、`--all` 裸推**（会把 GitHub 的 `refs/pull/*` 污染到 Gitee）。
- push 默认**不带 `--force`**；force 仅允许出现在用户显式确认后的单向对齐路径（见 safety 规则）。
- 解析 git 输出做状态判定时，覆盖设计文档 §4.3 的映射：`Everything up-to-date` → synced；`[rejected]` non-fast-forward → conflict；认证/网络失败 → error 并透出原始信息。

## 日志

- 统一 `tracing` 宏（`tracing::info!` / `warn!` / `error!`），带结构化字段（如 `tracing::info!(count = n, "扫描完成")`）。
- 禁止 `println!`、`eprintln!`、`dbg!` 进入提交代码。
- 敏感信息（token、完整凭证）**任何级别都不得写日志**。

## 网络请求

- HTTP 用 `reqwest`（已配置 rustls-tls，禁开 default features 引入 openssl）。
- API 端点固定：GitHub `https://api.github.com`，Gitee `https://gitee.com/api/v5`。
- 单测中不得打真实 API，用本地 mock 或抽象层验证（设计文档 §9）。

## 测试

- 领域模块（scanner/repo/sync/scheduler）必须带 `#[cfg(test)]` 单测，`cargo test` 全绿才算完成。
- sync 测试用 `tempfile` + 本地 bare 仓库伪造两端，覆盖：fast-forward 推送、tag 补推、分叉检测、tag 漂移拒推。
- 命令行行为验证在仓库根用 `cargo test`（实际执行目录 `src-tauri/`）。

## 格式

- `cargo fmt` 格式化；公共 API 写简短中文 doc 注释说明用途即可，不写废话注释。
