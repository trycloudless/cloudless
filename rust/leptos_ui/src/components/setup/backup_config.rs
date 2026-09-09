use crate::components::common::{tauri_invoke, tauri_invoke_no_args};
use crate::state::*;
use api_types::backup_config::{CreateBackupConfigResponse, CreateBackupConfigUiRequest};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn BackupConfigSetup() -> impl IntoView {
    let (session, set_session) = use_context::<SessionSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let (source_dir, set_source_dir) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        let s = session.get_untracked();

        let storage_id = match s.storage_id {
            Some(id) => id,
            None => {
                set_error.set(Some("No storage configured. Please go back.".to_string()));
                set_loading.set(false);
                return;
            }
        };

        let device_id = match s.local_device_id {
            Some(id) => id,
            None => {
                set_error.set(Some("No device registered. Please go back.".to_string()));
                set_loading.set(false);
                return;
            }
        };

        spawn_local(async move {
            let req = CreateBackupConfigUiRequest {
                storage_id,
                display_name: "Default Backup".to_string(),
                local_device_id: device_id,
                source_directory: source_dir.get_untracked(),
                cleanup_type: Default::default(),
                exclusion_config: Default::default(),
            };

            match tauri_invoke::<_, CreateBackupConfigResponse>(
                "create_backup_config",
                "request",
                req,
            )
            .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        s.config_id = Some(resp.id);
                        s.storage_configured = true;
                    });
                    is_unlocked.set(true);
                    set_phase.set(AppPhase::Main(MainView::Dashboard));
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    let readonly_input_class = "flex-1 bg-input border border-border rounded-xl px-4 py-3 text-text-primary cursor-default select-all focus:outline-none";
    let label_class = "block text-sm font-medium text-text-secondary mb-2";

    let on_set_up_later = move |_: leptos::ev::MouseEvent| {
        is_unlocked.set(true);
        set_phase.set(AppPhase::Main(MainView::Dashboard));
    };

    let pick_folder = move |_: leptos::ev::MouseEvent| {
        spawn_local(async move {
            match tauri_invoke_no_args::<Option<String>>("pick_directory").await {
                Ok(Some(path)) => set_source_dir.set(path),
                _ => {}
            }
        });
    };

    view! {
        <div>
            <form on:submit=on_submit>
                <h2 class="text-2xl md:text-3xl font-display font-bold gradient-text mb-2">"Choose what to protect"</h2>
                <p class="text-text-secondary mb-6 text-sm md:text-base">
                    "Choose the folder CloudLess should protect first. You can add more folders later."
                </p>

                <div class="mb-5">
                    <label class=label_class for="source-directory">"Folder to protect"</label>
                    <div class="flex gap-2">
                        // Read-only display — path is only set via the folder picker.
                        // on:input is retained so that E2E tests can set the value
                        // programmatically via dispatchEvent; the `readonly` attribute
                        // prevents direct user typing.
                        <input
                            type="text"
                            id="source-directory"
                            aria-label="Folder to protect"
                            data-testid="source-directory-input"
                            class=readonly_input_class
                            placeholder="No folder selected"
                            readonly
                            prop:value=move || source_dir.get()
                            on:input=move |ev| set_source_dir.set(event_target_value(&ev))
                        />
                        <button
                            type="button"
                            class="flex-shrink-0 px-4 py-3 bg-primary-tint border border-primary-glow rounded-xl text-primary text-sm font-medium hover:bg-primary-tint/80 transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                            on:click=pick_folder
                        >
                            "Choose Folder"
                        </button>
                    </div>
                </div>

                {move || error.try_get().flatten().map(|e| view! {
                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
                })}

                <button
                    type="submit"
                    class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                    disabled=move || loading.try_get().unwrap_or(false) || source_dir.get().is_empty()
                >
                    {move || if loading.try_get().unwrap_or(false) { "Setting up..." } else { "Complete Setup" }}
                </button>
            </form>

            <div class="mt-4 text-center">
                <button
                    type="button"
                    class="text-sm text-text-secondary hover:text-text-primary transition-colors"
                    on:click=on_set_up_later
                >
                    "Set up later"
                </button>
                <p class="text-xs text-text-secondary mt-1">
                    "You can finish setup without choosing a folder, but backup will not start until one is selected."
                </p>
            </div>
        </div>
    }
}
