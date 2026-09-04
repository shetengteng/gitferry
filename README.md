# GitFerry

A macOS menu bar app that scans local git repositories and keeps them in sync between GitHub and Gitee, automatically. No webhooks, no servers.

GitFerry 是一个 macOS 桌面应用：自动扫描本机所有 git 仓库，按仓库配置同步方向（GitHub ⇄ Gitee），后台定时自动同步——无需 webhook、无需服务器、复用系统 git 凭据。

## 技术栈

- Rust + Tauri 2
- Vue 3 + TypeScript + Pinia
- 同步引擎调 `git` CLI，凭证由系统钥匙串自理

## 开发

```bash
pnpm install
pnpm tauri dev      # 开发运行
cargo test          # Rust 单测（src-tauri 下）
pnpm tauri build    # 打包 .dmg
```

## 设计文档

[docs/github-gitee-sync-design.md](./docs/github-gitee-sync-design.md)
