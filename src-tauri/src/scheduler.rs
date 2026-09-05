//! 后台轮询调度器：常驻线程按间隔对启用仓库做限流并发同步。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::Manager;

use crate::auth::Platform;
use crate::commands::{self, SharedConfig, SharedSync};
use crate::config::{AccountStatus, RepoEntry, RepoKind};
use crate::sync::{RepoSyncState, SyncStatus};

/// 启动后首轮延迟（秒），避免开机自启抢跑
pub const FIRST_ROUND_DELAY_SECS: u64 = 30;
/// 单轮内 error 状态重试次数（safety 规则 13：重试 1 次）
pub const RETRY_TIMES: u32 = 1;
/// 连续失败退避阈值（safety 规则 12：3 次后退避）
pub const BACKOFF_THRESHOLD: u32 = 3;
/// 退避倍数
pub const BACKOFF_MULTIPLIER: u32 = 3;
/// 令牌低频校验间隔（分钟）
pub const TOKEN_VERIFY_INTERVAL_MINS: u32 = 360;

/// 单仓库调度档案（内存态，不落盘）
#[derive(Debug, Clone, Default)]
struct RepoSchedule {
    /// 连续失败次数：synced 清零；conflict/missing 不更新
    fail_streak: u32,
    /// 上次尝试时间（unix 秒）
    last_attempt: Option<i64>,
}

/// 调度器共享状态：轮询线程与托盘手动触发共用。
#[derive(Default)]
pub struct Scheduler {
    schedules: Mutex<HashMap<PathBuf, RepoSchedule>>,
    /// 防重入：调度轮与托盘手动触发互斥
    round_busy: AtomicBool,
    last_token_verify: Mutex<Option<Instant>>,
}

/// 锁内部 Mutex；中毒时取回内部数据继续（调度档案允许降级）。
fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// 当前 unix 秒；时钟异常时按 0 处理（有调度档案的仓库会被视为未到期，极边缘场景可忽略）。
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 启动常驻调度线程（setup 时调用一次），失败仅记录不 panic。
pub fn spawn(app: tauri::AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("gitferry-scheduler".into())
        .spawn(move || run_forever(app));
    if let Err(err) = spawned {
        tracing::error!(%err, "调度线程启动失败，自动同步不可用");
    }
}

/// 托盘「立即全量同步」入口：与轮询互斥，忙时忽略本次触发。
pub fn trigger_manual_round(app: &tauri::AppHandle) {
    let busy = &app.state::<Scheduler>().round_busy;
    match busy.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => {
            let app = app.clone();
            std::thread::spawn(move || {
                run_round(&app, false);
                app.state::<Scheduler>()
                    .round_busy
                    .store(false, Ordering::SeqCst);
            });
        }
        Err(_) => tracing::info!("同步进行中，忽略本次手动触发"),
    }
}

/// 对给定仓库做限流并发同步（分批 spawn + 顺序 join），每项结果即时写回运行时状态。
pub(crate) fn execute_entries(
    app: &tauri::AppHandle,
    entries: &[RepoEntry],
    concurrency: usize,
) -> Vec<(PathBuf, RepoSyncState)> {
    let concurrency = concurrency.max(1);
    let mut results = Vec::with_capacity(entries.len());
    for chunk in entries.chunks(concurrency) {
        let mut handles = Vec::with_capacity(chunk.len());
        for entry in chunk {
            commands::mark_syncing(&app.state::<SharedSync>(), &entry.path);
            let entry = entry.clone();
            let path = entry.path.clone();
            handles.push((path, std::thread::spawn(move || sync_with_retry(&entry))));
        }
        for (path, handle) in handles {
            let state = match handle.join() {
                Ok(state) => state,
                Err(_) => RepoSyncState {
                    status: SyncStatus::Error,
                    error: Some("同步任务异常终止".into()),
                    ..Default::default()
                },
            };
            commands::store_state(&app.state::<SharedSync>(), &path, state.clone());
            results.push((path, state));
        }
    }
    results
}

/// 执行一次同步；仅 error 状态按 RETRY_TIMES 重试（safety 规则 13），conflict/missing 不重试。
fn sync_with_retry(entry: &RepoEntry) -> RepoSyncState {
    let mut state = crate::sync::sync_repo(entry);
    for _ in 0..RETRY_TIMES {
        if state.status != SyncStatus::Error {
            break;
        }
        tracing::warn!(path = %entry.path.display(), "同步出错，重试一次");
        state = crate::sync::sync_repo(entry);
    }
    state
}

/// 执行一轮同步：筛选到期仓库 → 限流并发同步 → 更新调度档案 → 广播状态。
/// 不管理 round_busy（由调用方负责占用与释放）。
fn run_round(app: &tauri::AppHandle, respect_backoff: bool) {
    let (interval_mins, concurrency, enabled) = {
        let config_state = app.state::<SharedConfig>();
        let config = config_state.lock().expect("config poisoned");
        (
            config.settings.sync_interval_mins,
            config.settings.concurrency as usize,
            config
                .repos
                .iter()
                .filter(|r| r.enabled && r.kind != RepoKind::None)
                .cloned()
                .collect::<Vec<_>>(),
        )
    };
    let now = now_secs();
    let (states, schedules) = {
        let scheduler = app.state::<Scheduler>();
        let schedules = lock_or_recover(&scheduler.schedules).clone();
        let sync_state = app.state::<SharedSync>();
        let states = commands::lock_sync(&sync_state).clone();
        (states, schedules)
    };
    let selected = select_repos(
        &enabled,
        &states,
        &schedules,
        now,
        interval_mins,
        respect_backoff,
    );
    if selected.is_empty() {
        tracing::debug!("本轮无到期仓库");
        return;
    }
    let results = execute_entries(app, &selected, concurrency);
    // 更新调度档案：synced 清零失败连击，error 累加，conflict/missing 仅刷新时间
    {
        let scheduler = app.state::<Scheduler>();
        let mut sched_map = lock_or_recover(&scheduler.schedules);
        for (path, state) in &results {
            let schedule = sched_map.entry(path.clone()).or_default();
            match state.status {
                SyncStatus::Synced => {
                    schedule.fail_streak = 0;
                    schedule.last_attempt = Some(now);
                }
                SyncStatus::Error => {
                    schedule.fail_streak += 1;
                    schedule.last_attempt = Some(now);
                }
                _ => schedule.last_attempt = Some(now),
            }
        }
    }
    commands::emit_sync_states(app);
    let pushed: u32 = results.iter().map(|(_, s)| s.pushed_refs).sum();
    let deleted: u32 = results.iter().map(|(_, s)| s.deleted_refs).sum();
    tracing::info!(total = results.len(), pushed, deleted, "调度轮完成");
}

/// 手动全量同步的候选仓库：与自动轮同口径（跳过 conflict/missing/syncing 与无 remote
/// 仓库），仅忽略退避节奏。供 commands::sync_all 复用，保证两处「全量同步」行为一致。
pub(crate) fn select_manual(app: &tauri::AppHandle, entries: &[RepoEntry]) -> Vec<RepoEntry> {
    let (interval_mins, states) = {
        let sync_state = app.state::<SharedSync>();
        let states = commands::lock_sync(&sync_state).clone();
        let config_state = app.state::<SharedConfig>();
        let interval = config_state
            .lock()
            .expect("config poisoned")
            .settings
            .sync_interval_mins;
        (interval, states)
    };
    let schedules = {
        let scheduler = app.state::<Scheduler>();
        let guard = lock_or_recover(&scheduler.schedules);
        guard.clone()
    };
    select_repos(
        entries,
        &states,
        &schedules,
        now_secs(),
        interval_mins,
        false,
    )
}

fn run_forever(app: tauri::AppHandle) {
    std::thread::sleep(Duration::from_secs(FIRST_ROUND_DELAY_SECS));
    loop {
        maybe_verify_tokens(&app);
        // 每轮开头重读配置，间隔变更下一轮生效
        let interval_mins = {
            let config_state = app.state::<SharedConfig>();
            let config = config_state.lock().expect("config poisoned");
            config.settings.sync_interval_mins
        };
        let busy = &app.state::<Scheduler>().round_busy;
        if busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            run_round(&app, true);
            busy.store(false, Ordering::SeqCst);
        }
        std::thread::sleep(Duration::from_secs(interval_mins.max(1) as u64 * 60));
    }
}

/// 低频校验平台令牌有效性；失效账号置 Invalid 并持久化（绝不记录令牌内容）。
fn maybe_verify_tokens(app: &tauri::AppHandle) {
    {
        let scheduler = app.state::<Scheduler>();
        let mut last = lock_or_recover(&scheduler.last_token_verify);
        // Unexpected / 网络失败同样计入周期，避免反复打平台 API
        if let Some(t) = *last {
            if t.elapsed() < Duration::from_secs(TOKEN_VERIFY_INTERVAL_MINS as u64 * 60) {
                return;
            }
        }
        *last = Some(Instant::now());
    }
    let accounts = {
        let config_state = app.state::<SharedConfig>();
        let config = config_state.lock().expect("config poisoned");
        config.accounts.clone()
    };
    for (platform, account) in [
        (Platform::Github, &accounts.github),
        (Platform::Gitee, &accounts.gitee),
    ] {
        if account.status != AccountStatus::Connected {
            continue;
        }
        let token = match crate::auth::get_token(platform) {
            Ok(Some(token)) => token,
            Ok(None) => {
                tracing::warn!(platform = platform.as_str(), "钥匙串中无令牌，跳过校验");
                continue;
            }
            Err(err) => {
                tracing::warn!(platform = platform.as_str(), %err, "读取令牌失败，本轮跳过校验");
                continue;
            }
        };
        match tauri::async_runtime::block_on(crate::auth::verify(platform, &token)) {
            crate::auth::VerifyOutcome::Connected(login) => {
                tracing::debug!(platform = platform.as_str(), login = %login, "令牌校验通过");
            }
            crate::auth::VerifyOutcome::Unauthorized => {
                tracing::warn!(
                    platform = platform.as_str(),
                    "令牌已失效，请在账号页重新配置"
                );
                let config_state = app.state::<SharedConfig>();
                if let Err(err) = commands::mark_account_invalid(&config_state, platform) {
                    tracing::warn!(%err, "账号状态更新失败");
                }
            }
            crate::auth::VerifyOutcome::Unexpected(detail) => {
                tracing::info!(platform = platform.as_str(), detail = %detail, "令牌校验异常，稍后重试");
            }
        }
    }
}

/// 连续失败达到 BACKOFF_THRESHOLD 后返回退避倍数，否则 1。
fn backoff_multiplier(fail_streak: u32) -> u32 {
    if fail_streak >= BACKOFF_THRESHOLD {
        BACKOFF_MULTIPLIER
    } else {
        1
    }
}

/// 到期判定：从未尝试视为到期；否则 now 达到上次尝试 +（间隔 × 退避倍数）。
fn is_due(schedule: &RepoSchedule, interval_mins: u32, now: i64) -> bool {
    let Some(last) = schedule.last_attempt else {
        return true;
    };
    let due_at =
        last + (interval_mins as i64 * 60) * backoff_multiplier(schedule.fail_streak) as i64;
    now >= due_at
}

/// 筛选本轮应同步的仓库：已启用且 kind 非 None；跳过 conflict/missing/syncing
/// （无状态记录视作 pending 入选）；respect_backoff 时按退避节奏过滤，手动触发则全部入选。
fn select_repos(
    entries: &[RepoEntry],
    states: &HashMap<PathBuf, RepoSyncState>,
    schedules: &HashMap<PathBuf, RepoSchedule>,
    now: i64,
    interval_mins: u32,
    respect_backoff: bool,
) -> Vec<RepoEntry> {
    entries
        .iter()
        .filter(|entry| entry.enabled && entry.kind != RepoKind::None)
        .filter(|entry| {
            !matches!(
                states.get(&entry.path).map(|s| s.status),
                Some(SyncStatus::Conflict) | Some(SyncStatus::Missing) | Some(SyncStatus::Syncing)
            )
        })
        .filter(|entry| {
            if !respect_backoff {
                return true;
            }
            let schedule = schedules.get(&entry.path).cloned().unwrap_or_default();
            is_due(&schedule, interval_mins, now)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Direction, HostKind, RemoteInfo};

    fn entry(path: &str, enabled: bool, kind: RepoKind) -> RepoEntry {
        RepoEntry {
            path: PathBuf::from(path),
            enabled,
            direction: Direction::Both,
            remotes: vec![RemoteInfo {
                name: "origin".into(),
                url: "git@github.com:a/b.git".into(),
                host: HostKind::Github,
            }],
            kind,
            mirror_delete: false,
        }
    }

    fn state(status: SyncStatus) -> RepoSyncState {
        RepoSyncState {
            status,
            ..Default::default()
        }
    }

    #[test]
    fn backoff_multiplier_boundaries() {
        assert_eq!(backoff_multiplier(0), 1);
        assert_eq!(backoff_multiplier(1), 1);
        assert_eq!(backoff_multiplier(2), 1);
        assert_eq!(backoff_multiplier(BACKOFF_THRESHOLD), BACKOFF_MULTIPLIER);
        assert_eq!(backoff_multiplier(10), 3);
    }

    #[test]
    fn is_due_cases() {
        // 从未尝试 → 到期
        assert!(is_due(&RepoSchedule::default(), 10, 0));
        let ran = RepoSchedule {
            fail_streak: 0,
            last_attempt: Some(1_000),
        };
        // 普通间隔：10 分钟 = 600 秒
        assert!(!is_due(&ran, 10, 1_599));
        assert!(is_due(&ran, 10, 1_600));
        // 连续失败 3 次 → 退避 3 倍 = 1800 秒，到期点 1000 + 1800 = 2800
        let failing = RepoSchedule {
            fail_streak: 3,
            last_attempt: Some(1_000),
        };
        assert!(!is_due(&failing, 10, 2_799));
        assert!(is_due(&failing, 10, 2_800));
    }

    #[test]
    fn select_repos_filters() {
        let entries = vec![
            entry("/a", true, RepoKind::Both),
            entry("/b", false, RepoKind::Both), // 未启用
            entry("/c", true, RepoKind::None),  // 缺对端
        ];
        let mut states = HashMap::new();
        let mut schedules = HashMap::new();
        // conflict 跳过（自动与手动一致）
        states.insert(PathBuf::from("/a"), state(SyncStatus::Conflict));
        assert!(select_repos(&entries, &states, &schedules, 60, 10, true).is_empty());
        assert!(select_repos(&entries, &states, &schedules, 60, 10, false).is_empty());

        // error + 连续失败退避：手动触发忽略退避入选，自动轮未到期跳过
        states.insert(PathBuf::from("/a"), state(SyncStatus::Error));
        schedules.insert(
            PathBuf::from("/a"),
            RepoSchedule {
                fail_streak: 5,
                last_attempt: Some(0),
            },
        );
        assert!(select_repos(&entries, &states, &schedules, 60, 10, true).is_empty());
        assert_eq!(
            select_repos(&entries, &states, &schedules, 60, 10, false).len(),
            1
        );
        // 退避 3 倍 × 10 分钟 = 1800 秒后到期
        assert_eq!(
            select_repos(&entries, &states, &schedules, 1_801, 10, true).len(),
            1
        );

        // missing / syncing 跳过
        states.insert(PathBuf::from("/a"), state(SyncStatus::Missing));
        assert!(select_repos(&entries, &states, &schedules, 10_000, 10, true).is_empty());
        states.insert(PathBuf::from("/a"), state(SyncStatus::Syncing));
        assert!(select_repos(&entries, &states, &schedules, 10_000, 10, true).is_empty());

        // 无状态/调度记录视作 pending 且从未尝试 → 入选
        states.remove(&PathBuf::from("/a"));
        schedules.remove(&PathBuf::from("/a"));
        assert_eq!(
            select_repos(&entries, &states, &schedules, 0, 10, false).len(),
            1
        );
        assert_eq!(
            select_repos(&entries, &states, &schedules, 0, 10, true).len(),
            1
        );
    }
}
