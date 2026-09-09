use crate::state::*;
use leptos::prelude::*;

use super::backup_config::BackupConfigSetup;
use super::device_registration::DeviceRegistration;
use super::encryption_setup::EncryptionSetup;
use super::remote_storage_config::RemoteStorageConfig;

fn step_index(s: SetupStep) -> usize {
    match s {
        SetupStep::EncryptionPassword => 0,
        SetupStep::Device => 1,
        SetupStep::RemoteStorage => 2,
        SetupStep::BackupConfig => 3,
    }
}

#[component]
pub fn SetupWizard(step: SetupStep) -> impl IntoView {
    let steps = vec![
        ("Encryption", SetupStep::EncryptionPassword),
        ("Device", SetupStep::Device),
        ("Storage", SetupStep::RemoteStorage),
        ("Backup", SetupStep::BackupConfig),
    ];

    let current_idx = step_index(step);
    let total_steps = steps.len();
    let current_label = steps[current_idx].0;

    view! {
        <div class="flex items-center justify-center min-h-screen p-4 md:p-8">
            <div class="glass bg-surface border border-border rounded-3xl p-6 md:p-10 w-full max-w-lg shadow-2xl">
                // Step indicator
                <div class="md:hidden text-center text-xs font-medium text-text-secondary mb-3">
                    {format!("Step {} of {} — {}", current_idx + 1, total_steps, current_label)}
                </div>
                <div class="flex items-center justify-center gap-2 md:gap-3 mb-8">
                    {steps.into_iter().enumerate().map(|(i, (label, _s))| {
                        let is_active = i == current_idx;
                        let is_done = i < current_idx;
                        view! {
                            <div class="flex items-center gap-1 md:gap-2">
                                <div class=move || {
                                    if is_active {
                                        "w-8 h-8 rounded-full btn-gradient flex items-center justify-center text-sm font-bold"
                                    } else if is_done {
                                        "w-8 h-8 rounded-full bg-accent flex items-center justify-center text-sm font-bold"
                                    } else {
                                        "w-8 h-8 rounded-full bg-input border border-border flex items-center justify-center text-sm text-text-secondary"
                                    }
                                }>
                                    {if is_done { "\u{2713}".to_string() } else { (i + 1).to_string() }}
                                </div>
                                <span class=move || {
                                    let base = if is_active { "text-sm font-medium text-text-primary" }
                                    else if is_done { "text-sm font-medium text-accent" }
                                    else { "text-sm text-text-secondary" };
                                    format!("hidden md:inline {}", base)
                                }>
                                    {label}
                                </span>
                            </div>
                            {if i < total_steps - 1 {
                                view! { <div class="w-4 md:w-8 h-px bg-border"></div> }.into_any()
                            } else {
                                ().into_any()
                            }}
                        }
                    }).collect_view()}
                </div>

                {match step {
                    SetupStep::EncryptionPassword => view! { <EncryptionSetup /> }.into_any(),
                    SetupStep::Device => view! { <DeviceRegistration /> }.into_any(),
                    SetupStep::RemoteStorage => view! { <RemoteStorageConfig /> }.into_any(),
                    SetupStep::BackupConfig => view! { <BackupConfigSetup /> }.into_any(),
                }}
            </div>
        </div>
    }
}
