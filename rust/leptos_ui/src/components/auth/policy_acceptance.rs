use crate::components::common::{tauri_invoke, tauri_invoke_no_args};
use crate::state::*;
use api_types::policy::{
    AcceptPolicyRequest, AcceptPolicyResponse, GetPendingPoliciesResponse, PendingPolicyResponse,
    PolicyType, SkipPolicyRequest, SkipPolicyResponse,
};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

const WEBSITE_BASE_URL: &str = match option_env!("WEBSITE_BASE_URL") {
    Some(url) => url,
    None => "http://localhost:3000",
};

fn policy_slug(policy_type: &PolicyType) -> Option<&'static str> {
    match policy_type {
        PolicyType::TermsOfService => Some("terms-of-service"),
        PolicyType::PrivacyPolicy => Some("privacy-policy"),
        PolicyType::CookiePolicy => Some("cookie-policy"),
        PolicyType::AcceptableUse => Some("acceptable-use-policy"),
        PolicyType::RefundAndCancellation => Some("refund-and-cancellation-policy"),
        PolicyType::SecurityAndDataHandling => Some("security-and-data-handling"),
        PolicyType::Subprocessors => Some("subprocessors"),
        PolicyType::DataRetentionAndDeletion => Some("data-retention-and-deletion-policy"),
    }
}

#[component]
pub fn PolicyAcceptance() -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (session, _) = use_context::<SessionSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let navigate_after_policies = move || {
        if session.get_untracked().is_first_time_setup {
            set_phase.set(AppPhase::Setup(SetupStep::EncryptionPassword));
        } else {
            is_unlocked.set(true);
            set_phase.set(AppPhase::Main(MainView::Dashboard));
        }
    };

    let (policies, set_policies) = signal(Vec::<PendingPolicyResponse>::new());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);
    let (processing, set_processing) = signal(false);

    // Fetch pending policies on mount
    Effect::new(move || {
        spawn_local(async move {
            match tauri_invoke_no_args::<GetPendingPoliciesResponse>("get_pending_policies").await {
                Ok(response) => {
                    if response.list.is_empty() {
                        navigate_after_policies();
                    } else {
                        set_policies.set(response.list);
                        set_loading.set(false);
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to load policies: {}", e)));
                    set_loading.set(false);
                }
            }
        });
    });

    let on_accept = move |policy_version_id: uuid::Uuid| {
        set_processing.set(true);
        set_error.set(None);
        let physical_device_id = session.get_untracked().physical_device_id.clone();

        spawn_local(async move {
            let req = AcceptPolicyRequest {
                policy_version_id,
                ip_address: None,
                user_agent: None,
                physical_device_id: Some(physical_device_id),
            };

            match tauri_invoke::<_, AcceptPolicyResponse>("accept_policy", "request", req).await {
                Ok(_) => {
                    set_policies.update(|list| {
                        list.retain(|p| p.policy_version_id != policy_version_id);
                    });
                    if policies.get_untracked().is_empty() {
                        navigate_after_policies();
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to accept policy: {}", e)));
                }
            }
            set_processing.set(false);
        });
    };

    let on_skip = move |policy_version_id: uuid::Uuid| {
        set_processing.set(true);
        set_error.set(None);
        let physical_device_id = session.get_untracked().physical_device_id.clone();

        spawn_local(async move {
            let req = SkipPolicyRequest {
                policy_version_id,
                ip_address: None,
                user_agent: None,
                physical_device_id: Some(physical_device_id),
            };

            match tauri_invoke::<_, SkipPolicyResponse>("skip_policy", "request", req).await {
                Ok(_) => {
                    set_policies.update(|list| {
                        list.retain(|p| p.policy_version_id != policy_version_id);
                    });
                    if policies.get_untracked().is_empty() {
                        navigate_after_policies();
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to skip policy: {}", e)));
                }
            }
            set_processing.set(false);
        });
    };

    view! {
        <div class="min-h-screen flex items-center justify-center bg-bg p-6">
            <div class="w-full max-w-2xl">
                <h2 class="text-3xl font-display font-bold gradient-text mb-2 text-center">
                    "Before you continue"
                </h2>
                <p class="text-text-secondary mb-8 text-center">
                    {move || if session.get().is_first_time_setup {
                        "To complete your account setup, please review and accept the following policies."
                    } else {
                        "Our policies have been updated. Please review and accept them to continue using Cloudless."
                    }}
                </p>

                {move || error.get().map(|e| view! {
                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">
                        {e}
                    </div>
                })}

                {move || {
                    if loading.get() {
                        view! {
                            <div class="text-center text-text-secondary">"Loading policies..."</div>
                        }.into_any()
                    } else {
                        let current_policies = policies.get();
                        view! {
                            <div class="space-y-6">
                                {current_policies.into_iter().map(|policy| {
                                    let version_id = policy.policy_version_id;
                                    let can_skip = policy.max_skips > 0 && policy.times_skipped < policy.max_skips;
                                    let remaining = policy.max_skips - policy.times_skipped;

                                    view! {
                                        <div class="bg-surface border border-border rounded-2xl p-6">
                                            <div class="flex items-center justify-between mb-4">
                                                <h3 class="text-xl font-semibold text-text-primary">
                                                    {policy.policy_title.clone()}
                                                </h3>
                                                <span class="text-sm text-text-secondary">
                                                    {format!("Version {}", policy.version)}
                                                </span>
                                            </div>

                                            <div class="max-h-64 overflow-y-auto bg-bg rounded-xl p-4 mb-4 text-sm text-text-secondary whitespace-pre-wrap border border-border-alpha">
                                                {policy.content.clone()}
                                            </div>

                                            {if let Some(slug) = policy_slug(&policy.policy_type) {
                                                let url = format!("{}/legal/{}", WEBSITE_BASE_URL, slug);
                                                view! {
                                                    <div class="mb-3">
                                                        <a
                                                            href=url
                                                            target="_blank"
                                                            rel="noopener noreferrer"
                                                            class="text-xs text-primary-light hover:text-text-primary transition-colors"
                                                        >
                                                            "View full policy ↗"
                                                        </a>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <span /> }.into_any()
                                            }}

                                            <div class="flex items-center gap-3 justify-end">
                                                {if can_skip {
                                                    let on_skip = on_skip.clone();
                                                    view! {
                                                        <button
                                                            class="px-4 py-2 rounded-xl text-sm text-text-secondary border border-border hover:bg-bg transition-all disabled:opacity-50"
                                                            disabled=move || processing.get()
                                                            on:click=move |_| on_skip(version_id)
                                                        >
                                                            {format!("Skip for now ({} left)", remaining)}
                                                        </button>
                                                    }.into_any()
                                                } else {
                                                    view! { <span /> }.into_any()
                                                }}
                                                <button
                                                    class="btn-gradient text-white px-6 py-2 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                                                    disabled=move || processing.get()
                                                    on:click={
                                                        let on_accept = on_accept.clone();
                                                        move |_| on_accept(version_id)
                                                    }
                                                >
                                                    "I Accept"
                                                </button>
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
