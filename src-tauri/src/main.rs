#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
#[cfg(test)]
mod updater_tests;
mod updates;
use easy_switch_core::Store;
use tauri::Manager;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = args
        .iter()
        .position(|a| a == "--store")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(feature = "test-channel") {
                std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                    .join("EasySwitchTest")
            } else {
                Store::default_root()
            }
        });
    if args.iter().any(|a| a == "--runtime") {
        let result = Store::open(root).and_then(|store| {
            tokio::runtime::Runtime::new()?.block_on(easy_switch_core::runtime::serve(store))
        });
        if result.is_err() {
            std::process::exit(1);
        }
        return;
    }
    let store = Store::open(root).expect("无法打开 Easy Switch 数据目录");
    let migration = if args.iter().any(|a| a == "--store") {
        None
    } else {
        store
            .migrate(
                &std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                    .join("CodexSwitch"),
            )
            .err()
            .map(|e| e.to_string())
    };
    let state = commands::AppState::new(store, migration);
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Old versions saved native decorations; the new titlebar owns the caption.
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::DECORATIONS)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .manage(updates::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::save_provider,
            commands::delete_provider,
            commands::save_settings,
            commands::discover,
            commands::switch_provider,
            commands::batch_sessions,
            commands::scan_sessions,
            commands::session_detail,
            commands::usage_report,
            commands::export_sessions,
            commands::open_thread,
            commands::doctor,
            commands::fetch_models,
            commands::cancel_operation,
            commands::list_operations,
            commands::restore_operation,
            commands::save_scroll,
            updates::check_update,
            updates::download_update,
            updates::install_update
        ])
        .setup(|app| {
            let store = app.state::<commands::AppState>().store.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let (Ok(s), Ok(ps)) = (store.settings(), store.providers()) {
                    if ps
                        .iter()
                        .any(|p| Some(&p.id) == s.active_provider_id.as_ref() && p.runtime())
                    {
                        let _ = easy_switch_core::runtime::ensure_started_blocking(&store);
                    }
                }
            });
            updates::schedule(app.handle().clone());
            Ok(())
        })
        .on_window_event(|w, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = w.state::<commands::AppState>();
                if state.busy.load(std::sync::atomic::Ordering::SeqCst) {
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("无法启动 Easy Switch");
}
