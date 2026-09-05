//! 菜单栏托盘：全局状态图标 + 快捷菜单。

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

use crate::commands::{self, SharedSync};
use crate::sync::{RepoSyncState, SyncStatus};

/// 托盘聚合状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Idle,
    Syncing,
    Conflict,
    Error,
}

/// 托盘句柄：状态行菜单项，供 update 更新文案。
pub struct TrayHandles {
    pub status_item: MenuItem<tauri::Wry>,
}

/// 聚合全部仓库状态：syncing 优先，其次 conflict，再次 error/missing，否则 idle。
pub fn aggregate(map: &HashMap<PathBuf, RepoSyncState>) -> TrayStatus {
    let mut has_conflict = false;
    let mut has_error = false;
    for state in map.values() {
        match state.status {
            SyncStatus::Syncing => return TrayStatus::Syncing,
            SyncStatus::Conflict => has_conflict = true,
            SyncStatus::Error | SyncStatus::Missing => has_error = true,
            SyncStatus::Pending | SyncStatus::Synced => {}
        }
    }
    if has_conflict {
        TrayStatus::Conflict
    } else if has_error {
        TrayStatus::Error
    } else {
        TrayStatus::Idle
    }
}

/// 冲突与出错（missing 并入出错）仓库计数。
fn counts(map: &HashMap<PathBuf, RepoSyncState>) -> (u32, u32) {
    let mut conflict = 0;
    let mut error = 0;
    for state in map.values() {
        match state.status {
            SyncStatus::Conflict => conflict += 1,
            SyncStatus::Error | SyncStatus::Missing => error += 1,
            _ => {}
        }
    }
    (conflict, error)
}

/// 托盘状态行文案（菜单第一项，禁用态仅作展示）。
fn status_line(status: TrayStatus, conflict_n: u32, error_n: u32) -> String {
    match status {
        TrayStatus::Idle => "就绪".into(),
        TrayStatus::Syncing => "同步中…".into(),
        TrayStatus::Conflict => format!("{conflict_n} 个冲突待处理"),
        TrayStatus::Error => format!("{error_n} 个出错"),
    }
}

/// 托盘提示文案。
fn tooltip_for(status: TrayStatus, conflict_n: u32, error_n: u32) -> String {
    format!("GitFerry · {}", status_line(status, conflict_n, error_n))
}

fn idle_icon() -> tauri::image::Image<'static> {
    tauri::include_image!("icons/tray/idle.png")
}

fn syncing_icon() -> tauri::image::Image<'static> {
    tauri::include_image!("icons/tray/syncing.png")
}

fn conflict_icon() -> tauri::image::Image<'static> {
    tauri::include_image!("icons/tray/conflict.png")
}

fn error_icon() -> tauri::image::Image<'static> {
    tauri::include_image!("icons/tray/error.png")
}

fn icon_for(status: TrayStatus) -> tauri::image::Image<'static> {
    match status {
        TrayStatus::Idle => idle_icon(),
        TrayStatus::Syncing => syncing_icon(),
        TrayStatus::Conflict => conflict_icon(),
        TrayStatus::Error => error_icon(),
    }
}

/// 初始化托盘（setup 时调用一次）：状态行 + 打开主窗口 / 立即全量同步 / 退出。
/// 不启用 icon_as_template，保持彩色圆点图标。
pub fn init(app: &tauri::App) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "就绪", false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "打开主窗口", true, None::<&str>)?;
    let sync_all = MenuItem::with_id(app, "sync_all", "立即全量同步", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .item(&status)
        .separator()
        .item(&show)
        .item(&sync_all)
        .separator()
        .item(&quit)
        .build()?;
    TrayIconBuilder::with_id("gitferry-tray")
        .icon(idle_icon())
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("GitFerry · 就绪")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "sync_all" => crate::scheduler::trigger_manual_round(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    app.manage(TrayHandles {
        status_item: status,
    });
    Ok(())
}

/// 依据当前运行时同步状态刷新托盘图标、提示与状态行（emit_sync_states 调用）。
pub fn update(app: &tauri::AppHandle) {
    let map = {
        let sync_state = app.state::<SharedSync>();
        let guard = commands::lock_sync(&sync_state);
        guard.clone()
    };
    let status = aggregate(&map);
    let (conflict_n, error_n) = counts(&map);
    let tray = match app.tray_by_id("gitferry-tray") {
        Some(tray) => tray,
        None => {
            tracing::warn!("托盘未初始化，跳过状态刷新");
            return;
        }
    };
    if let Err(err) = tray.set_icon(Some(icon_for(status))) {
        tracing::warn!(%err, "托盘图标更新失败");
    }
    if let Err(err) = tray.set_tooltip(Some(tooltip_for(status, conflict_n, error_n))) {
        tracing::warn!(%err, "托盘提示更新失败");
    }
    if let Some(handles) = app.try_state::<TrayHandles>() {
        if let Err(err) = handles
            .status_item
            .set_text(status_line(status, conflict_n, error_n))
        {
            tracing::warn!(%err, "托盘状态行更新失败");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(status: SyncStatus) -> RepoSyncState {
        RepoSyncState {
            status,
            ..Default::default()
        }
    }

    #[test]
    fn aggregate_priorities() {
        let mut map = HashMap::new();
        assert_eq!(aggregate(&map), TrayStatus::Idle);
        map.insert(PathBuf::from("/a"), state(SyncStatus::Synced));
        assert_eq!(aggregate(&map), TrayStatus::Idle);
        map.insert(PathBuf::from("/b"), state(SyncStatus::Missing));
        assert_eq!(aggregate(&map), TrayStatus::Error);
        map.insert(PathBuf::from("/c"), state(SyncStatus::Conflict));
        assert_eq!(aggregate(&map), TrayStatus::Conflict);
        map.insert(PathBuf::from("/d"), state(SyncStatus::Syncing));
        assert_eq!(aggregate(&map), TrayStatus::Syncing);
    }

    #[test]
    fn counts_include_missing() {
        let mut map = HashMap::new();
        map.insert(PathBuf::from("/a"), state(SyncStatus::Conflict));
        map.insert(PathBuf::from("/b"), state(SyncStatus::Missing));
        map.insert(PathBuf::from("/c"), state(SyncStatus::Error));
        map.insert(PathBuf::from("/d"), state(SyncStatus::Synced));
        assert_eq!(counts(&map), (1, 2));
    }

    #[test]
    fn lines_and_tooltips() {
        assert_eq!(status_line(TrayStatus::Idle, 0, 0), "就绪");
        assert_eq!(status_line(TrayStatus::Syncing, 1, 2), "同步中…");
        assert_eq!(status_line(TrayStatus::Conflict, 2, 0), "2 个冲突待处理");
        assert_eq!(status_line(TrayStatus::Error, 0, 1), "1 个出错");
        assert_eq!(tooltip_for(TrayStatus::Idle, 0, 0), "GitFerry · 就绪");
        assert_eq!(
            tooltip_for(TrayStatus::Conflict, 2, 3),
            "GitFerry · 2 个冲突待处理"
        );
        assert_eq!(tooltip_for(TrayStatus::Error, 0, 1), "GitFerry · 1 个出错");
    }
}
