mod auth;
mod commands;
mod config;
mod error;
mod logging;
mod repo;
mod scanner;
mod scheduler;
mod sync;
mod tray;

pub fn run() {
    logging::init();
    let config = config::Config::load().unwrap_or_else(|err| {
        tracing::warn!(%err, "配置加载失败，使用默认配置");
        config::Config::default()
    });
    let app = tauri::Builder::default()
        // single-instance 必须最先注册：二次启动时唤起已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(commands::SharedConfig::new(config))
        .manage(commands::SharedSync::new(std::collections::HashMap::new()))
        .manage(scheduler::Scheduler::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::scan_repos,
            commands::set_repo_config,
            commands::set_repos_config,
            commands::set_repo_mirror,
            commands::save_settings,
            commands::reveal_logs_dir,
            commands::configure_account,
            commands::complete_setup,
            commands::sync_now,
            commands::sync_all,
            commands::get_sync_states,
            commands::resolve_conflict,
        ])
        // 菜单栏常驻：点关闭只隐藏窗口，不退出
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            tray::init(app)?;
            scheduler::spawn(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    // macOS Dock/Finder 重新点击应用图标（applicationShouldHandleReopen）时唤起主窗口
    app.run(|_app, event| {
        if let tauri::RunEvent::Reopen { .. } = event {
            use tauri::Manager;
            if let Some(w) = _app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    });
}
