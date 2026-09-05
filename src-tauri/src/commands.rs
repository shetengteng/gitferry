use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::auth::{self, Platform};
use crate::config::{Account, Accounts, Config, Direction, RepoEntry, Settings};
use crate::error::{FerryError, FerryResult};
use crate::sync::{self, ConflictSide, RepoSyncResult, RepoSyncState, SyncStatus};
use crate::{repo, scanner};

pub type SharedConfig = Mutex<Config>;

/// 运行时同步状态（仓库路径 → 状态），仅内存不落盘
pub type SharedSync = Mutex<HashMap<PathBuf, RepoSyncState>>;

#[derive(Debug, Deserialize)]
pub struct RepoConfigInput {
    pub path: String,
    pub enabled: bool,
    pub direction: Direction,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStatePayload {
    pub settings: Settings,
    pub repos: Vec<RepoEntry>,
    pub accounts: Accounts,
    pub setup_done: bool,
}

fn payload(config: &Config) -> AppStatePayload {
    AppStatePayload {
        settings: config.settings.clone(),
        repos: config.repos.clone(),
        accounts: config.accounts.clone(),
        setup_done: config.setup_done,
    }
}

fn mutate<T>(
    state: &State<'_, SharedConfig>,
    f: impl FnOnce(&mut Config) -> FerryResult<T>,
) -> FerryResult<T> {
    let mut config = state.lock().expect("config poisoned");
    let result = f(&mut config)?;
    config.save()?;
    Ok(result)
}

#[tauri::command]
pub fn get_app_state(state: State<'_, SharedConfig>) -> AppStatePayload {
    payload(&state.lock().expect("config poisoned"))
}

#[tauri::command]
pub fn scan_repos(state: State<'_, SharedConfig>) -> FerryResult<Vec<RepoEntry>> {
    mutate(&state, |config| {
        let paths = scanner::scan(&config.settings.scan_roots, config.settings.max_depth);
        let mut merged: Vec<RepoEntry> = paths
            .into_iter()
            .map(|path| {
                let remotes = repo::inspect_repo(&path);
                let kind = repo::kind_for(&remotes);
                match config.repos.iter().find(|r| r.path == path) {
                    Some(existing) => RepoEntry {
                        path,
                        enabled: existing.enabled,
                        direction: existing.direction,
                        remotes,
                        kind,
                    },
                    None => RepoEntry {
                        path,
                        enabled: false,
                        direction: Direction::Both,
                        remotes,
                        kind,
                    },
                }
            })
            .collect();
        // 扫描未覆盖但已配置的仓库保留（missing 状态由同步引擎标记）
        for old in &config.repos {
            if !merged.iter().any(|m| m.path == old.path) {
                merged.push(old.clone());
            }
        }
        let result = merged.clone();
        config.repos = merged;
        tracing::info!(count = result.len(), "扫描完成");
        Ok(result)
    })
}

#[tauri::command]
pub fn set_repo_config(
    state: State<'_, SharedConfig>,
    path: String,
    enabled: bool,
    direction: Direction,
) -> FerryResult<()> {
    mutate(&state, |config| {
        let entry = config
            .repos
            .iter_mut()
            .find(|r| r.path == PathBuf::from(&path))
            .ok_or_else(|| FerryError::msg(format!("仓库未收录：{path}")))?;
        entry.enabled = enabled;
        entry.direction = direction;
        Ok(())
    })
}

#[tauri::command]
pub fn set_repos_config(
    state: State<'_, SharedConfig>,
    items: Vec<RepoConfigInput>,
) -> FerryResult<()> {
    mutate(&state, |config| {
        for item in &items {
            let entry = config
                .repos
                .iter_mut()
                .find(|r| r.path == PathBuf::from(&item.path))
                .ok_or_else(|| FerryError::msg(format!("仓库未收录：{}", item.path)))?;
            entry.enabled = item.enabled;
            entry.direction = item.direction;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, SharedConfig>,
    scan_roots: Vec<String>,
    max_depth: u32,
    sync_interval_mins: u32,
    concurrency: u32,
) -> FerryResult<Settings> {
    mutate(&state, |config| {
        config.settings = Settings {
            scan_roots: scan_roots.into_iter().map(PathBuf::from).collect(),
            max_depth,
            sync_interval_mins,
            concurrency,
        };
        Ok(config.settings.clone())
    })
}

/// 在 Finder 中显示日志目录（macOS `open -R`），便于用户反馈问题时取日志。
#[tauri::command]
pub fn reveal_logs_dir() -> FerryResult<()> {
    let dir = crate::logging::logs_dir();
    std::fs::create_dir_all(&dir)?;
    std::process::Command::new("open")
        .arg("-R")
        .arg(&dir)
        .spawn()
        .map_err(|err| FerryError::msg(format!("无法打开日志目录：{err}")))?;
    Ok(())
}

#[tauri::command]
pub async fn configure_account(
    state: State<'_, SharedConfig>,
    platform: Platform,
    token: String,
) -> FerryResult<Account> {
    let account = auth::configure(platform, &token).await?;
    mutate(&state, |config| {
        match platform {
            Platform::Github => config.accounts.github = account.clone(),
            Platform::Gitee => config.accounts.gitee = account.clone(),
        }
        Ok(account)
    })
}

#[tauri::command]
pub fn complete_setup(state: State<'_, SharedConfig>) -> FerryResult<()> {
    mutate(&state, |config| {
        config.setup_done = true;
        Ok(())
    })
}

// ---------- 同步（M2）：逻辑在 sync.rs，command 层只做状态读写与调度 ----------

/// 锁同步状态表；中毒时取回内部数据继续（运行时状态允许降级）。
fn lock_sync<'a>(
    sync: &'a State<'_, SharedSync>,
) -> MutexGuard<'a, HashMap<PathBuf, RepoSyncState>> {
    match sync.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn find_entry(config: &Config, path: &str) -> FerryResult<RepoEntry> {
    config
        .repos
        .iter()
        .find(|r| r.path == PathBuf::from(path))
        .cloned()
        .ok_or_else(|| {
            FerryError::msg(format!(
                "仓库未收录：{path}。请先执行一次扫描，或检查仓库路径。"
            ))
        })
}

fn ensure_enabled(entry: &RepoEntry, path: &str) -> FerryResult<()> {
    if entry.enabled {
        Ok(())
    } else {
        Err(FerryError::msg(format!(
            "仓库未启用同步：{path}。请先在仓库列表打开同步开关。"
        )))
    }
}

/// 进入 syncing 状态；保留上次同步时间与未裁决冲突，清空上次错误。
fn mark_syncing(sync: &State<'_, SharedSync>, path: &Path) {
    let mut map = lock_sync(sync);
    let prev = map.get(path).cloned();
    map.insert(
        path.to_path_buf(),
        RepoSyncState {
            status: SyncStatus::Syncing,
            last_synced: prev.as_ref().and_then(|s| s.last_synced),
            pushed_refs: 0,
            error: None,
            conflicts: prev.map(|s| s.conflicts).unwrap_or_default(),
        },
    );
}

fn store_state(sync: &State<'_, SharedSync>, path: &Path, state: RepoSyncState) {
    lock_sync(sync).insert(path.to_path_buf(), state);
}

#[tauri::command]
pub async fn sync_now(
    state: State<'_, SharedConfig>,
    sync: State<'_, SharedSync>,
    path: String,
) -> FerryResult<RepoSyncState> {
    let entry = {
        let config = state.lock().expect("config poisoned");
        find_entry(&config, &path)?
    };
    ensure_enabled(&entry, &path)?;
    let repo_path = entry.path.clone();
    mark_syncing(&sync, &repo_path);
    // 阻塞式 git 子进程调用放进 blocking 线程池，避免卡住异步运行时
    let join = tauri::async_runtime::spawn_blocking(move || sync::sync_repo(&entry));
    let new_state = match join.await {
        Ok(result) => result,
        Err(err) => {
            let msg = format!("同步任务执行失败：{err}");
            store_state(
                &sync,
                &repo_path,
                RepoSyncState {
                    status: SyncStatus::Error,
                    error: Some(msg.clone()),
                    ..Default::default()
                },
            );
            return Err(FerryError::msg(msg));
        }
    };
    store_state(&sync, &repo_path, new_state.clone());
    Ok(new_state)
}

#[tauri::command]
pub async fn sync_all(
    state: State<'_, SharedConfig>,
    sync: State<'_, SharedSync>,
) -> FerryResult<Vec<RepoSyncResult>> {
    let (enabled, all_paths) = {
        let config = state.lock().expect("config poisoned");
        let enabled: Vec<RepoEntry> = config.repos.iter().filter(|r| r.enabled).cloned().collect();
        let all_paths: Vec<String> = config
            .repos
            .iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .collect();
        (enabled, all_paths)
    };
    let enabled_paths: Vec<PathBuf> = enabled.iter().map(|e| e.path.clone()).collect();
    for path in &enabled_paths {
        mark_syncing(&sync, path);
    }
    // M2 顺序执行；并发限流由 M3 调度器引入
    let join = tauri::async_runtime::spawn_blocking(move || {
        enabled
            .iter()
            .map(|entry| (entry.path.clone(), sync::sync_repo(entry)))
            .collect::<Vec<_>>()
    });
    let fresh = match join.await {
        Ok(results) => results,
        Err(err) => {
            let msg = format!("同步任务执行失败：{err}");
            {
                let mut map = lock_sync(&sync);
                for path in &enabled_paths {
                    map.insert(
                        path.clone(),
                        RepoSyncState {
                            status: SyncStatus::Error,
                            error: Some(msg.clone()),
                            ..Default::default()
                        },
                    );
                }
            }
            return Err(FerryError::msg(msg));
        }
    };
    for (path, sync_state) in &fresh {
        store_state(&sync, path, sync_state.clone());
    }
    // 返回全部仓库：已启用的取最新结果，未启用的保持既有状态（无记录按 pending）
    let mut results = Vec::with_capacity(all_paths.len());
    {
        let map = lock_sync(&sync);
        for p in &all_paths {
            let sync_state = map.get(&PathBuf::from(p)).cloned().unwrap_or_default();
            results.push(RepoSyncResult {
                path: p.clone(),
                state: sync_state,
            });
        }
    }
    tracing::info!(total = results.len(), "全量同步完成");
    Ok(results)
}

#[tauri::command]
pub fn get_sync_states(sync: State<'_, SharedSync>) -> Vec<RepoSyncResult> {
    let map = lock_sync(&sync);
    map.iter()
        .map(|(path, state)| RepoSyncResult {
            path: path.to_string_lossy().into_owned(),
            state: state.clone(),
        })
        .collect()
}

/// 用户在 UI 显式确认后的冲突裁决（三选一）：按权威侧 force 对齐两端 remote，
/// 随后重新执行一次同步。这是全项目唯一允许 --force 的路径。
#[tauri::command]
pub async fn resolve_conflict(
    state: State<'_, SharedConfig>,
    sync: State<'_, SharedSync>,
    path: String,
    side: ConflictSide,
) -> FerryResult<RepoSyncState> {
    let entry = {
        let config = state.lock().expect("config poisoned");
        find_entry(&config, &path)?
    };
    let conflicts = {
        let map = lock_sync(&sync);
        match map.get(&PathBuf::from(&path)) {
            Some(existing) if !existing.conflicts.is_empty() => existing.conflicts.clone(),
            _ => {
                return Err(FerryError::msg(format!(
                    "仓库当前没有待裁决的冲突：{path}。请先执行同步刷新冲突列表。"
                )));
            }
        }
    };
    mark_syncing(&sync, &entry.path);
    let task_conflicts = conflicts.clone();
    let join = tauri::async_runtime::spawn_blocking(move || -> FerryResult<RepoSyncState> {
        let pushed = sync::resolve_conflicts(&entry, &task_conflicts, side)?;
        tracing::info!(path = %entry.path.display(), pushed, "冲突裁决完成，重新同步");
        Ok(sync::sync_repo(&entry))
    });
    let new_state = match join.await {
        Ok(Ok(result)) => result,
        Ok(Err(err)) => {
            // 裁决失败保留冲突列表，用户可重试或换一侧
            store_state(
                &sync,
                &PathBuf::from(&path),
                RepoSyncState {
                    status: SyncStatus::Error,
                    last_synced: None,
                    pushed_refs: 0,
                    error: Some(err.to_string()),
                    conflicts,
                },
            );
            return Err(err);
        }
        Err(err) => {
            let msg = format!("冲突裁决任务执行失败：{err}");
            store_state(
                &sync,
                &PathBuf::from(&path),
                RepoSyncState {
                    status: SyncStatus::Error,
                    error: Some(msg.clone()),
                    conflicts,
                    ..Default::default()
                },
            );
            return Err(FerryError::msg(msg));
        }
    };
    store_state(&sync, &PathBuf::from(&path), new_state.clone());
    Ok(new_state)
}
