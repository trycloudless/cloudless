pub mod bootstrap;
pub mod commands;
pub mod error;
pub mod google_auth;
pub mod models;
pub mod onedrive_auth;

pub use commands::*;
pub use models::AppState;

use cloudless_core::{adapters::sqlite_local_index::SqliteLocalIndex, app_env::AppEnv};
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Runtime env var overrides build-time default from .env
    let api_base =
        std::env::var("API_BASE_URL").unwrap_or_else(|_| env!("API_BASE_URL").to_string());
    let base_url = url::Url::parse(&api_base).expect("Invalid API_BASE_URL");

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    #[cfg(feature = "e2e")]
    {
        builder = builder.plugin(tauri_plugin_webdriver::init());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
        // Hide to tray on window close instead of quitting so the background
        // scheduler keeps running. The tray "Quit" menu item is the exit path.
        builder = builder.on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        });
    }

    builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .level_for("tauri_lib", log::LevelFilter::Debug)
                .level_for("cloudless_core", log::LevelFilter::Debug)
                .level_for("hyper_util", log::LevelFilter::Warn)
                .level_for("hyper", log::LevelFilter::Warn)
                .level_for("reqwest", log::LevelFilter::Warn)
                .level_for("h2", log::LevelFilter::Warn)
                .level_for("sqlx", log::LevelFilter::Warn)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_folder_picker::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_biometry::init())
        .invoke_handler(tauri::generate_handler![
            check_for_updates,
            install_update,
            create_user,
            signup_and_login,
            login,
            verify_email,
            resend_verification,
            setup_encryption,
            unlock_encryption,
            register_local_device,
            get_local_device,
            get_or_create_device,
            register_remote_storage,
            create_backup_config,
            list_backup_configs,
            list_all_backup_configs,
            toggle_backup_config,
            rename_backup_config,
            update_cleanup_type,
            preview_backup_exclusions,
            update_backup_exclusions,
            list_all_devices,
            resolve_mobile_device,
            confirm_mobile_device,
            get_hostname,
            get_machine_id,
            get_platform,
            list_remote_storages,
            add_remote_storage,
            complete_storage_setup,
            test_s3_connection,
            add_local_storage,
            test_local_connection,
            test_sftp_connection,
            add_sftp_storage,
            add_google_drive_storage,
            reauth_google_drive_storage,
            add_onedrive_storage,
            reauth_onedrive_storage,
            start_backup_command,
            list_backed_up_files,
            move_version_to_bin,
            move_all_versions_to_bin,
            restore_version_from_bin,
            list_bin_versions,
            list_backup_jobs,
            get_backup_job_detail,
            get_latest_backup_job,
            abandon_stale_backup_jobs,
            get_resumable_job_for_config,
            get_dashboard_stats,
            get_subscription,
            start_restore_command,
            list_restore_jobs,
            get_restore_job_detail,
            get_latest_restore_job,
            pick_directory,
            resolve_standard_dir,
            open_path_in_explorer,
            open_file,
            open_restored_file,
            open_billing_portal,
            open_checkout,
            get_checkout_status,
            enable_auto_backup,
            disable_auto_backup,
            get_auto_backup_settings,
            get_checkout_policy,
            get_pending_policies,
            accept_policy,
            skip_policy,
            export_recovery_key,
            rotate_recovery_key,
            has_recovery_key,
            unlock_with_recovery_key,
            change_password_after_recovery,
            change_encryption_password,
            is_biometric_available,
            is_biometric_enabled,
            enable_biometric_unlock,
            biometric_unlock,
            disable_biometric_unlock,
            run_garbage_collection,
            list_gc_runs,
            get_gc_run_detail,
            get_retention_settings,
            update_retention_settings,
            enable_start_at_login,
            disable_start_at_login,
            is_start_at_login_enabled,
            set_tray_configs,
            set_tray_status,
        ])
        .setup(move |app| {
            use tauri::Manager;

            // Determine the writable data directory.
            // CLOUDLESS_DATA_DIR env var overrides the default, useful for e2e tests
            // to isolate test data from the real user database.
            // Desktop default: data_local_dir()/cloudless.
            // Mobile: dirs is not usable — use Tauri's app data dir for sandbox storage.
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let db_dir = std::env::var("CLOUDLESS_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    dirs::data_local_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("cloudless")
                });
            #[cfg(any(target_os = "android", target_os = "ios"))]
            let db_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Failed to resolve app data dir on mobile: {e}"))?;
            let db_path = db_dir.join("local_index.db");

            // Open the local SQLite index for client-side file tracking.
            let local_index = tauri::async_runtime::block_on(async {
                SqliteLocalIndex::open(&db_path)
                    .await
                    .expect("Failed to open local SQLite index")
            });

            let env = Arc::new(AppEnv::new(base_url, local_index));

            app.manage(AppState {
                env,
                dek_cache: Arc::new(RwLock::new(None)),
                derived_keys_cache: Arc::new(RwLock::new(None)),
                device_id_cache: Arc::new(RwLock::new(None)),
                scheduler_handle: Arc::new(RwLock::new(None)),
            });

            // System tray: full menu built at startup with current autostart state.
            // Configs and last-backup status are added dynamically after the user unlocks.
            // The TrayIcon handle is stored in TrayState so commands can rebuild the menu.
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                use crate::commands::{build_tray_menu, rebuild_tray_menu};
                use crate::models::{NavigateToConfigPayload, TrayState};
                use tauri::tray::TrayIconBuilder;
                use tauri_plugin_autostart::ManagerExt;

                let start_at_login = app.autolaunch().is_enabled().unwrap_or(false);
                let initial_menu = build_tray_menu(app.handle(), &[], None, start_at_login)?;

                let tray = TrayIconBuilder::new()
                    .menu(&initial_menu)
                    .show_menu_on_left_click(true)
                    .icon(app.default_window_icon().unwrap().clone())
                    .on_menu_event(|app: &tauri::AppHandle, event| {
                        use tauri::{Emitter, Manager};
                        match event.id.as_ref() {
                            "open" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                            "quit" => app.exit(0),
                            "backup-now" => {
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    let state = app.state::<AppState>();
                                    let dek = state.dek_cache.read().await.clone();
                                    let derived_keys =
                                        state.derived_keys_cache.read().await.clone();
                                    let device_id = *state.device_id_cache.read().await;
                                    let (Some(dek), Some(derived_keys), Some(device_id)) =
                                        (dek, derived_keys, device_id)
                                    else {
                                        tracing::warn!(
                                            "Tray: backup-now skipped — encryption not unlocked"
                                        );
                                        return;
                                    };
                                    let Ok(physical_device_id) = machine_uid::get() else {
                                        return;
                                    };
                                    let env = (*state.env).clone();
                                    let (event_tx, mut event_rx) =
                                        tokio::sync::mpsc::channel(32);
                                    let app2 = app.clone();
                                    tokio::spawn(async move {
                                        while let Some(ev) = event_rx.recv().await {
                                            use tauri::Emitter;
                                            let _ = app2.emit("scheduler-event", &ev);
                                        }
                                    });
                                    cloudless_core::applications::backup::scheduler::run_backup_cycle(
                                        &env,
                                        &dek,
                                        &derived_keys,
                                        device_id,
                                        &physical_device_id,
                                        &event_tx,
                                    )
                                    .await;
                                });
                            }
                            "toggle-start-at-login" => {
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    use tauri_plugin_autostart::ManagerExt;
                                    let enabled =
                                        app.autolaunch().is_enabled().unwrap_or(false);
                                    if enabled {
                                        let _ = app.autolaunch().disable();
                                    } else {
                                        let _ = app.autolaunch().enable();
                                    }
                                    if let Err(e) = rebuild_tray_menu(&app).await {
                                        tracing::error!(
                                            "Tray: failed to rebuild after autostart toggle: {e}"
                                        );
                                    }
                                });
                            }
                            id => {
                                // Config items use their UUID as the menu-item ID.
                                if let Ok(config_id) = uuid::Uuid::parse_str(id) {
                                    let _ = app.emit(
                                        "navigate-to-config",
                                        NavigateToConfigPayload { config_id },
                                    );
                                    if let Some(w) = app.get_webview_window("main") {
                                        let _ = w.show();
                                        let _ = w.set_focus();
                                    }
                                }
                            }
                        }
                    })
                    .build(app)?;

                app.manage(TrayState {
                    icon: Arc::new(RwLock::new(Some(tray))),
                    configs: Arc::new(RwLock::new(vec![])),
                    last_backup_label: Arc::new(RwLock::new(None)),
                });
            }

            // On Android, initialize rustls-platform-verifier with the JVM context
            // so reqwest can perform TLS certificate verification using OS roots.
            #[cfg(target_os = "android")]
            {
                app.get_webview_window("main")
                    .expect("main window not found")
                    .with_webview(|webview| {
                        webview.jni_handle().exec(|env, context, _webview| {
                            use tauri::wry::prelude::JObject;
                            let loader = env
                                .call_method(
                                    context,
                                    "getClassLoader",
                                    "()Ljava/lang/ClassLoader;",
                                    &[],
                                )
                                .expect("Failed to get class loader");
                            rustls_platform_verifier::android::init_with_refs(
                                env.get_java_vm().expect("Failed to get JavaVM"),
                                env.new_global_ref(context)
                                    .expect("Failed to create global ref for context"),
                                env.new_global_ref(
                                    JObject::try_from(loader)
                                        .expect("Failed to convert loader to JObject"),
                                )
                                .expect("Failed to create global ref for class loader"),
                            );
                        });
                    })?;
            }

            // On mobile, restore security-scoped access to previously bookmarked folders
            #[cfg(mobile)]
            {
                if let Some(picker) =
                    app.try_state::<tauri_plugin_folder_picker::FolderPicker<tauri::Wry>>()
                {
                    if let Err(e) = picker.restore_access() {
                        tracing::warn!("Failed to restore folder access: {}", e);
                    }
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // Re-show the main window when the macOS dock icon is clicked
            // while all windows are hidden (app is living in the tray).
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows, .. } = &event {
                if !*has_visible_windows {
                    use tauri::Manager;
                    if let Some(w) = _app_handle.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
            let _ = event;
        });
}
