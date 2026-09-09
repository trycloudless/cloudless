use crate::components::common::{tauri_invoke, tauri_invoke_no_args};
use crate::state::SessionSignal;
use api_types::policy::{AcceptPolicyRequest, GetCheckoutPolicyResponse, PendingPolicyResponse};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Shown before checkout opens. Fetches the Refund & Cancellation policy for the user:
/// - If already accepted: calls `on_close(true)` immediately (no UI shown).
/// - If pending: shows the policy with "I Accept" and "Cancel" buttons.
/// - "I Accept" writes to user_policy_acceptances, then calls `on_close(true)`.
/// - "Cancel" / backdrop click calls `on_close(false)`.
#[component]
pub fn CheckoutPolicyModal(
    /// `true` = accepted (proceed to checkout), `false` = cancelled.
    on_close: Callback<bool>,
) -> impl IntoView {
    let (session, _) = use_context::<SessionSignal>().unwrap();

    // None = loading, Some(None) = no policy needed, Some(Some(_)) = policy to show
    let (policy, set_policy) = signal(Option::<Option<PendingPolicyResponse>>::None);
    let (accepting, set_accepting) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    Effect::new(move || {
        spawn_local(async move {
            match tauri_invoke_no_args::<GetCheckoutPolicyResponse>("get_checkout_policy").await {
                Ok(resp) => {
                    if resp.policy.is_none() {
                        // Already accepted — skip the modal and proceed
                        on_close.run(true);
                    } else {
                        set_policy.set(Some(resp.policy));
                    }
                }
                Err(e) => {
                    set_policy.set(Some(None)); // show nothing but surface the error
                    set_error.set(Some(format!("Failed to load policy: {}", e)));
                }
            }
        });
    });

    let on_accept = move |policy_version_id: uuid::Uuid| {
        set_accepting.set(true);
        set_error.set(None);
        let physical_device_id = session.get_untracked().physical_device_id.clone();

        spawn_local(async move {
            let req = AcceptPolicyRequest {
                policy_version_id,
                ip_address: None,
                user_agent: None,
                physical_device_id: Some(physical_device_id),
            };
            match tauri_invoke::<_, api_types::policy::AcceptPolicyResponse>(
                "accept_policy",
                "request",
                req,
            )
            .await
            {
                Ok(_) => on_close.run(true),
                Err(e) => {
                    set_error.set(Some(format!("Failed to accept policy: {}", e)));
                    set_accepting.set(false);
                }
            }
        });
    };

    view! {
        {move || {
            // Still loading — show backdrop with spinner
            if policy.get().is_none() {
                return view! {
                    <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
                        <div class="text-text-secondary text-sm">"Loading…"</div>
                    </div>
                }.into_any();
            }

            let p = match policy.get().flatten() {
                Some(p) => p,
                None => return view! { <div /> }.into_any(), // error state, error shown below
            };

            let version_id = p.policy_version_id;

            view! {
                <div
                    class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4"
                    on:click=move |_| on_close.run(false)
                >
                    <div
                        class="glass bg-surface border border-border rounded-2xl w-full max-w-lg animate-slide-up"
                        on:click=move |ev: leptos::ev::MouseEvent| ev.stop_propagation()
                    >
                        // Header
                        <div class="flex items-center justify-between p-5 border-b border-border">
                            <h3 class="text-lg font-semibold">{p.policy_title.clone()}</h3>
                            <button
                                class="text-text-secondary hover:text-text-primary transition-colors"
                                on:click=move |_| on_close.run(false)
                            >
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </div>

                        // Policy content
                        <div class="p-5">
                            <p class="text-sm text-text-secondary mb-4">
                                "Please review our Refund & Cancellation policy before upgrading."
                            </p>

                            {error.get().map(|e| view! {
                                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-4">
                                    {e}
                                </div>
                            })}

                            <div class="max-h-64 overflow-y-auto bg-bg rounded-xl p-4 text-sm text-text-secondary whitespace-pre-wrap border border-border-alpha">
                                {p.content.clone()}
                            </div>
                        </div>

                        // Footer
                        <div class="flex items-center justify-end gap-3 p-5 border-t border-border">
                            <button
                                class="px-4 py-2 rounded-xl text-sm text-text-secondary border border-border hover:bg-bg transition-all disabled:opacity-50"
                                disabled=move || accepting.get()
                                on:click=move |_| on_close.run(false)
                            >
                                "Cancel"
                            </button>
                            <button
                                class="btn-gradient text-white px-6 py-2 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                                disabled=move || accepting.get()
                                on:click=move |_| on_accept(version_id)
                            >
                                {move || if accepting.get() { "Accepting…" } else { "I Accept" }}
                            </button>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
