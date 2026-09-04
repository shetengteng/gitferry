use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::auth::{self, Platform};
use crate::config::{Account, Accounts, Config, Direction, RepoEntry, Settings};
use crate::error::{FerryError, FerryResult};
use crate::{repo, scanner};

pub type SharedConfig = Mutex<Config>;

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
) -> FerryResult<Settings> {
    mutate(&state, |config| {
        config.settings = Settings {
            scan_roots: scan_roots.into_iter().map(PathBuf::from).collect(),
            max_depth,
        };
        Ok(config.settings.clone())
    })
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
