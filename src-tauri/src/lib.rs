mod auth;
mod commands;
mod config;
mod error;
mod logging;
mod repo;
mod scanner;
mod sync;

pub fn run() {
    logging::init();
    let config = config::Config::load().unwrap_or_else(|err| {
        tracing::warn!(%err, "配置加载失败，使用默认配置");
        config::Config::default()
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::SharedConfig::new(config))
        .manage(commands::SharedSync::new(std::collections::HashMap::new()))
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::scan_repos,
            commands::set_repo_config,
            commands::set_repos_config,
            commands::save_settings,
            commands::reveal_logs_dir,
            commands::configure_account,
            commands::complete_setup,
            commands::sync_now,
            commands::sync_all,
            commands::get_sync_states,
            commands::resolve_conflict,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
