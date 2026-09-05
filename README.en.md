# GitFerry

[简体中文](./README.md) | English

GitFerry is a macOS menu bar app that scans your local git repositories and keeps them in sync between GitHub and Gitee, in the direction you choose per repo — no webhooks, no servers, and it reuses your system git credentials.

## Features

- **Automatic repo discovery**: point it at scan root directories (with configurable depth) and it finds your existing git repos for you — no manual adding one by one.
- **Bidirectional sync**: each repo can sync independently — GitHub → Gitee, Gitee → GitHub, or both.
- **One-click sync / sync all**: trigger manually at any time; per-repo status badges reflect results in real time (synced / conflict / error).
- **Explicit conflict handling**: when both sides have new commits, the repo is marked as conflicted and you choose — keep GitHub / keep Gitee / manual merge. GitFerry never picks a side for you.
- **Tokens never touch disk**: GitHub / Gitee PATs are stored only in the macOS Keychain (service name `gitferry`); `config.json` contains no tokens.
- **Reuses system git credentials**: syncing runs through the system `git` CLI, so credentials are handled by the Keychain (osxkeychain) — the app never manages your passwords.
- **Safe push boundary**: only explicit refspecs are pushed (`refs/heads/<branch>`, `refs/tags/<tag>`), never `--mirror` / `--all`; automatic paths are fast-forward only, with no auto force-push and no ref deletion.
- **Local logs**: rolling file logs, openable from the settings in one click.

## Usage

### 1. Install

Build from source (see [Development](#development)) or install the `.dmg` produced by `pnpm tauri build`.

### 2. First-run setup (wizard)

1. Set your scan roots (e.g. `~/code`) and scan depth;
2. Connect GitHub / Gitee accounts: paste a PAT (Personal Access Token); the app verifies it against the platform API and stores it in the Keychain;
3. Finish the wizard to enter the main view.

> You can sync without configuring a PAT too — as long as you have pushed to that repo from the terminal once, the system Keychain already holds the credentials.

### 3. Pick repos and directions

- The main view lists discovered repos with their remote ownership (GitHub / Gitee / both);
- Enable the repos you want synced and choose each one's direction;
- Use "Sync now" for a single repo or "Sync all".

### 4. Resolve conflicts

When the same branch has new commits on both ends, the repo is marked **conflict** and alignment stops. Click the conflicted repo and choose one of three options:

- Keep GitHub (reset Gitee to GitHub's state);
- Keep Gitee (reset GitHub to Gitee's state);
- Manual merge (handle it yourself in the terminal, then resume syncing).

### Platform limits

- Gitee: single files over 100 MB fail to push; free repo quota is ~1 GB; Git-LFS objects are not supported.
- A repo whose path has moved or been deleted is marked `missing`; its config is kept and never auto-deleted.

## Development

```bash
pnpm install          # Install frontend dependencies
pnpm tauri dev        # Run in development (vite + cargo)
cargo test            # Rust unit tests (run under src-tauri)
pnpm build            # Frontend type check (vue-tsc) + build
pnpm tauri build      # Package .dmg
```

### Tech stack

- **Backend**: Rust + Tauri 2 (`git` CLI via `std::process::Command`, PATs via keyring, account verification via reqwest)
- **Frontend**: Vue 3 + TypeScript + Pinia + Tailwind CSS + shadcn-vue
- **Build**: Vite 6 + pnpm

### Roadmap

- [x] M1: scanner, repo detection, account connection, config persistence
- [x] M2: sync engine (fetch/push, tag catch-up, divergence detection, conflict resolution)
- [ ] M3: polling scheduler (scheduled auto-sync), launch at login

## Design Doc

The single source of truth for the sync algorithm and safety boundaries: [design/github-gitee-sync-design.md](./design/github-gitee-sync-design.md)

## License

MIT
