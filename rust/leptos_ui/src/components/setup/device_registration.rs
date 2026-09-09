use crate::components::common::{
    DeviceResolutionResult, format_local_date, tauri_invoke, tauri_invoke_no_args,
};
use crate::state::*;
use api_types::local_device::{DeviceSummary, GetOrCreateLocalDeviceResponse};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Represents the resolution state for mobile device registration.
#[derive(Debug, Clone)]
enum MobileResolutionState {
    Loading,
    MultipleFound(Vec<DeviceSummary>),
    /// No device found, or user chose "This is a new device" from the picker.
    NoneFound,
}

/// Navigates to the correct phase after device registration completes.
/// Returning users go to Dashboard; new users continue the setup wizard.
fn navigate_after_registration(
    returning_user: bool,
    set_phase: &WriteSignal<AppPhase>,
    is_unlocked: IsUnlockedSignal,
) {
    if returning_user {
        is_unlocked.set(true);
        set_phase.set(AppPhase::Main(MainView::Dashboard));
    } else {
        set_phase.set(AppPhase::Setup(SetupStep::RemoteStorage));
    }
}

/// Helper to update session state after a device is resolved.
fn apply_device_to_session(set_session: &WriteSignal<SessionInfo>, device: &DeviceSummary) {
    let device = device.clone();
    set_session.update(|s| {
        s.device_display_name = device.display_name;
        s.local_device_id = Some(device.id);
        s.physical_device_id = device.physical_device_id;
        s.device_registered = true;
        s.encryption_ready_banner = false;
    });
}

fn is_mobile_platform(platform: &str) -> bool {
    platform == "android" || platform == "ios"
}

#[component]
pub fn DeviceRegistration() -> impl IntoView {
    let (session, _) = use_context::<SessionSignal>().unwrap();
    let platform = session.get_untracked().device_platform.clone();

    let body = if is_mobile_platform(&platform) {
        MobileDeviceRegistration().into_any()
    } else {
        DesktopDeviceRegistration().into_any()
    };

    view! {
        {move || session.get().encryption_ready_banner.then(|| view! {
            <div class="mb-4 rounded-xl border border-success-tint bg-success-tint px-4 py-3 text-sm text-success">
                "Encryption is ready on this device."
            </div>
        })}
        {body}
    }
}

/// Desktop device registration — same as the original flow.
/// Shows a form with hostname pre-filled, user clicks Register.
#[component]
fn DesktopDeviceRegistration() -> impl IntoView {
    let (_, set_session) = use_context::<SessionSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let (display_name, set_display_name) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    // Auto-fetch hostname as default display name
    spawn_local({
        let set_display_name = set_display_name.clone();
        async move {
            if let Ok(name) = tauri_invoke_no_args::<String>("get_hostname").await {
                let _ = set_display_name.try_set(name);
            }
        }
    });

    let on_register = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match tauri_invoke::<_, GetOrCreateLocalDeviceResponse>(
                "get_or_create_device",
                "display_name",
                display_name.get_untracked(),
            )
            .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        s.device_display_name = resp.display_name.clone();
                        s.local_device_id = Some(resp.id);
                        s.device_registered = true;
                        s.encryption_ready_banner = false;
                    });
                    set_phase.set(AppPhase::Setup(SetupStep::RemoteStorage));
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    view! {
        <form on:submit=on_register>
            <h2 class="text-3xl font-display font-bold gradient-text mb-2">"Register Device"</h2>
            <p class="text-text-secondary mb-6">"Register this device to start backing up"</p>

            <div class="mb-5">
                <label class="block text-sm font-medium text-text-secondary mb-2" for="device-name">"Device Name"</label>
                <input
                    type="text"
                    id="device-name"
                    aria-label="Device Name"
                    class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                    prop:value=move || display_name.try_get().unwrap_or_default()
                    on:input=move |ev| set_display_name.set(event_target_value(&ev))
                    required
                />
            </div>

            {move || error.try_get().flatten().map(|e| view! {
                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
            })}

            <button
                type="submit"
                class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                disabled=move || loading.try_get().unwrap_or(false)
            >
                {move || if loading.try_get().unwrap_or(false) { "Registering..." } else { "Register Device" }}
            </button>
        </form>
    }
}

/// Mobile device registration — interactive resolution flow.
/// Automatically resolves the device if possible, otherwise shows a picker or new-device form.
#[component]
fn MobileDeviceRegistration() -> impl IntoView {
    let (session, set_session) = use_context::<SessionSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let (state, set_state) = signal(MobileResolutionState::Loading);
    let (error, set_error) = signal(Option::<String>::None);

    let returning_user = session.get_untracked().returning_user;

    // On mount, call resolve_mobile_device
    spawn_local({
        let set_state = set_state.clone();
        let set_session = set_session.clone();
        let set_phase = set_phase.clone();
        let set_error = set_error.clone();
        async move {
            match tauri_invoke_no_args::<DeviceResolutionResult>("resolve_mobile_device").await {
                Ok(DeviceResolutionResult::Resolved { device }) => {
                    apply_device_to_session(&set_session, &device);
                    navigate_after_registration(returning_user, &set_phase, is_unlocked);
                }
                Ok(DeviceResolutionResult::MultipleFound { devices }) => {
                    set_state.set(MobileResolutionState::MultipleFound(devices));
                }
                Ok(DeviceResolutionResult::NoneFound) => {
                    set_state.set(MobileResolutionState::NoneFound);
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                    set_state.set(MobileResolutionState::NoneFound);
                }
            }
        }
    });

    view! {
        {move || error.try_get().flatten().map(|e| view! {
            <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
        })}

        {move || match state.get() {
            MobileResolutionState::Loading => {
                view! {
                    <div class="text-center py-8">
                        <h2 class="text-3xl font-display font-bold gradient-text mb-2">"Detecting Device"</h2>
                        <p class="text-text-secondary">"Looking up your device..."</p>
                    </div>
                }.into_any()
            }
            MobileResolutionState::MultipleFound(ref devices) => {
                let devices = devices.clone();
                view! {
                    <MobileDevicePicker devices=devices set_parent_state=set_state />
                }.into_any()
            }
            MobileResolutionState::NoneFound => {
                view! {
                    <MobileNewDeviceForm />
                }.into_any()
            }
        }}
    }
}

/// Dropdown picker for selecting from multiple matching devices.
/// Includes a "This is a new device" option.
#[component]
fn MobileDevicePicker(
    devices: Vec<DeviceSummary>,
    set_parent_state: WriteSignal<MobileResolutionState>,
) -> impl IntoView {
    let (session, set_session) = use_context::<SessionSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let returning_user = session.get_untracked().returning_user;
    let (selected, set_selected) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let devices_for_lookup = devices.clone();

    let device_options = devices
        .iter()
        .map(|d| {
            let id = d.id.to_string();
            let label = d.display_name.clone().unwrap_or_else(|| {
                format!("{} ({})", d.platform, format_local_date(&d.created_at))
            });
            view! {
                <option value={id}>{label}</option>
            }
        })
        .collect_view();

    let on_confirm = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let selected_id = selected.get_untracked();

        if selected_id == "__new__" {
            // Transition parent state to NoneFound, which renders the new device form
            set_parent_state.set(MobileResolutionState::NoneFound);
            return;
        }

        let device = devices_for_lookup
            .iter()
            .find(|d| d.id.to_string() == selected_id)
            .cloned();

        let Some(device) = device else {
            let _ = set_error.try_set(Some("Please select a device".to_string()));
            return;
        };

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match tauri_invoke::<_, ()>(
                "confirm_mobile_device",
                "physical_device_id",
                device.physical_device_id.clone(),
            )
            .await
            {
                Ok(()) => {
                    apply_device_to_session(&set_session, &device);
                    navigate_after_registration(returning_user, &set_phase, is_unlocked);
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                    let _ = set_loading.try_set(false);
                }
            }
        });
    };

    view! {
        <form on:submit=on_confirm>
            <h2 class="text-3xl font-display font-bold gradient-text mb-2">"Select Device"</h2>
            <p class="text-text-secondary mb-6">"Multiple devices found on this platform. Please select yours."</p>

            <div class="mb-5">
                <label class="block text-sm font-medium text-text-secondary mb-2">"Your Device"</label>
                <select
                    class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                    on:change=move |ev| set_selected.set(event_target_value(&ev))
                    required
                >
                    <option value="" disabled selected>"Choose a device..."</option>
                    {device_options}
                    <option value="__new__">"This is a new device"</option>
                </select>
            </div>

            {move || error.try_get().flatten().map(|e| view! {
                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
            })}

            <button
                type="submit"
                class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                disabled=move || loading.try_get().unwrap_or(false)
            >
                {move || if loading.try_get().unwrap_or(false) { "Confirming..." } else { "Continue" }}
            </button>
        </form>
    }
}

/// Form for registering a brand new mobile device.
#[component]
fn MobileNewDeviceForm() -> impl IntoView {
    let (session, set_session) = use_context::<SessionSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let returning_user = session.get_untracked().returning_user;
    let (display_name, set_display_name) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_register = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match tauri_invoke::<_, GetOrCreateLocalDeviceResponse>(
                "get_or_create_device",
                "display_name",
                display_name.get_untracked(),
            )
            .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        s.device_display_name = resp.display_name.clone();
                        s.local_device_id = Some(resp.id);
                        s.physical_device_id = resp.physical_device_id.clone();
                        s.device_registered = true;
                    });
                    navigate_after_registration(returning_user, &set_phase, is_unlocked);
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    view! {
        <form on:submit=on_register>
            <h2 class="text-3xl font-display font-bold gradient-text mb-2">"New Device"</h2>
            <p class="text-text-secondary mb-6">"Give this device a name to identify it"</p>

            <div class="mb-5">
                <label class="block text-sm font-medium text-text-secondary mb-2">"Device Name"</label>
                <input
                    type="text"
                    placeholder="e.g. My iPhone"
                    class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                    prop:value=move || display_name.try_get().unwrap_or_default()
                    on:input=move |ev| set_display_name.set(event_target_value(&ev))
                    required
                />
            </div>

            {move || error.try_get().flatten().map(|e| view! {
                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
            })}

            <button
                type="submit"
                class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                disabled=move || loading.try_get().unwrap_or(false)
            >
                {move || if loading.try_get().unwrap_or(false) { "Registering..." } else { "Register Device" }}
            </button>
        </form>
    }
}
