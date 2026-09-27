//! ComInspect desktop application (Tauri shell).

mod commands;
mod error;
mod lifecycle;
mod state;
mod updates;
mod usage;

use tauri::{Manager, RunEvent};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

fn log_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let level = std::env::var("COMINSPECT_LOG")
        .ok()
        .and_then(|l| l.parse().ok())
        .unwrap_or(if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::LogDir {
            file_name: Some("cominspect".into()),
        }))
        .target(Target::new(TargetKind::Stdout))
        .level(level)
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("hyper_util", log::LevelFilter::Warn)
        .max_file_size(2_000_000)
        .rotation_strategy(RotationStrategy::KeepSome(3))
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        // A second launch focuses the running instance instead of opening a
        // second window that would compete for the same database.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }
    let app = builder
        .plugin(log_plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            // Show fresh "in use by" information when the user comes back.
            if let tauri::WindowEvent::Focused(true) = event
                && let Some(state) = window.app_handle().try_state::<state::AppState>()
            {
                state.usage.refresh();
            }
        })
        .setup(|app| {
            #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            let state = state::AppState::initialize(app.handle())?;
            app.manage(state);
            state::start_monitor(app.handle());
            updates::spawn_background_checks(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_inventory,
            commands::refresh,
            commands::get_device_detail,
            commands::update_identity,
            commands::set_ignored,
            commands::merge_devices,
            commands::forget_device,
            commands::export_inventory,
            commands::import_inventory,
            commands::get_app_info,
            commands::app_ready,
            commands::get_ui_prefs,
            commands::set_ui_prefs,
            commands::log_from_ui,
            commands::open_location,
            commands::open_link,
            commands::get_update_status,
            commands::check_for_updates,
            commands::install_update,
            commands::reinstall_version,
            commands::set_auto_update_check,
            commands::set_update_channel,
            commands::restart_app,
            commands::list_database_backups,
            commands::create_database_backup,
            commands::restore_database_backup,
            commands::diag_probe_catalog,
            commands::diag_open_test,
            commands::diag_cat_query,
            commands::diag_ptt_test,
            commands::get_port_usage,
            commands::watch_port,
            commands::unwatch_port,
            commands::get_watch_program,
            commands::pick_program,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ComInspect");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            state::stop_monitor(handle);
            log::info!("ComInspect exiting");
        }
    });
}
