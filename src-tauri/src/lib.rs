mod auth;
mod commands;
mod config;
mod error;
mod logging;
mod repo;
mod scanner;

pub fn run() {
    logging::init();
    let config = config::Config::load().unwrap_or_else(|err| {
        tracing::warn!(%err, "配置加载失败，使用默认配置");
        config::Config::default()
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::SharedConfig::new(config))
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::scan_repos,
            commands::set_repo_config,
            commands::save_settings,
            commands::configure_account,
            commands::complete_setup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
