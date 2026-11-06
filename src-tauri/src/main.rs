#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod acp;
mod develop;
mod devin_cli;
mod hooks;
mod pty;
mod research;
mod secrets;
mod skills;
mod state;
mod tasks;
mod workers;

use state::AppState;
use tauri::Manager;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::new())
        .setup(|app| {
            // auto-start the hook receiver when enabled in settings
            let state = app.state::<AppState>();
            let (enabled, port) = {
                let s = state.settings.lock();
                (s.auto_install_hooks, s.hook_port)
            };
            if enabled {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let st = handle.state::<AppState>();
                    if let Err(e) = hooks::start_server(
                        &st,
                        handle.clone(),
                        (port != 0).then_some(port),
                    )
                    .await
                    {
                        tracing::warn!("hook server autostart failed: {e}");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // devin cli
            devin_cli::devin_info,
            devin_cli::devin_auth_status,
            devin_cli::devin_login,
            devin_cli::url_open,
            devin_cli::devin_models,
            devin_cli::devin_print_once,
            devin_cli::pick_folder,
            // acp
            acp::acp_spawn,
            acp::acp_call,
            acp::acp_notify,
            acp::acp_session_new,
            acp::acp_session_load,
            acp::acp_prompt,
            acp::acp_cancel,
            acp::acp_permission_respond,
            acp::acp_kill,
            acp::acp_list,
            // pty
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            // workers / orchestration
            workers::worker_create,
            workers::worker_prompt,
            workers::worker_list,
            workers::worker_stop,
            workers::worker_delete,
            // research pipeline
            research::research_start,
            research::research_status,
            // develop pipeline
            develop::develop_start,
            develop::develop_status,
            // hooks
            hooks::hook_server_start,
            hooks::hook_server_stop,
            hooks::hooks_inject,
            // tasks board
            tasks::tasks_load,
            tasks::tasks_save,
            // secrets (windows credential manager)
            secrets::secret_set,
            secrets::secret_exists,
            secrets::secret_delete,
            // skills
            skills::skills_scan_local,
            skills::skills_github_search,
            skills::skills_install,
            skills::skills_set_enabled,
            // settings
            state::settings_get,
            state::settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Devil of Devin");
}
