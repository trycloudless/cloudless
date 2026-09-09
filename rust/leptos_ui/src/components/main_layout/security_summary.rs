use crate::state::*;
use leptos::prelude::*;

// ── SVG Icons ──────────────────────────────────────────────────────

const ICON_SHIELD_CHECK: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><polyline points="9 12 11 14 15 10"/></svg>"#;

const ICON_ALERT_TRIANGLE: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>"#;

// ── Security Status Summary ────────────────────────────────────────

/// Reusable security posture summary — shows encryption, key, storage, and device status.
/// Uses hardcoded architecture facts and session signals for dynamic status.
#[component]
pub fn SecurityStatusSummary() -> impl IntoView {
    let (session, _) = use_context::<SessionSignal>().unwrap();
    let (expanded, set_expanded) = signal(false);

    let verified_count = move || {
        let s = session.get();
        let mut count = 3u8; // encryption, key derivation, key storage always verified
        if s.storage_configured {
            count += 1;
        }
        if s.device_registered {
            count += 1;
        }
        count
    };

    view! {
        <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-4 md:p-5 mb-6">
            // Header — always visible
            <button
                class="w-full flex items-center justify-between md:cursor-default"
                on:click=move |_| set_expanded.update(|v| *v = !*v)
                aria-expanded=move || expanded.get().to_string()
            >
                <div class="flex items-center gap-3">
                    <div class="w-9 h-9 rounded-lg bg-success-tint flex items-center justify-center text-success"
                        inner_html=ICON_SHIELD_CHECK
                    ></div>
                    <div class="text-left">
                        <div class="text-sm font-semibold">"Protection Status"</div>
                        <div class="text-xs text-text-secondary">
                            {move || {
                                let vc = verified_count();
                                if vc == 5 {
                                    "Backup ready".to_string()
                                } else {
                                    format!("{}/5 verified", vc)
                                }
                            }}
                        </div>
                    </div>
                </div>
                // Chevron — visible on mobile only
                <svg
                    class=move || format!(
                        "w-4 h-4 text-text-secondary md:hidden transition-transform {}",
                        if expanded.get() { "rotate-180" } else { "" }
                    )
                    fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"
                >
                    <polyline points="6 9 12 15 18 9" />
                </svg>
            </button>

            // Detail rows — always visible on desktop, toggle on mobile
            <div class=move || format!(
                "mt-3 pt-3 border-t border-elevation-1-border space-y-2 {} md:block",
                if expanded.get() { "block" } else { "hidden" }
            )>
                // Static rows — always verified
                <SecurityRowStatic label="Encryption" detail="AES-256-GCM" />
                <SecurityRowStatic label="Key Derivation" detail="Argon2id" />
                <SecurityRowStatic label="Key Storage" detail="Local Device Only" />

                // Dynamic rows — depend on session state
                {move || {
                    let s = session.get();
                    let storage_ok = s.storage_configured;
                    let device_ok = s.device_registered;
                    view! {
                        <SecurityRowDynamic label="Storage" detail=if storage_ok { "Configured" } else { "Not configured" } verified=storage_ok />
                        <SecurityRowDynamic label="Device" detail=if device_ok { "Registered" } else { "Not registered" } verified=device_ok />
                    }
                }}
            </div>
        </div>
    }
}

/// A security row that is always green/verified (static architecture fact).
#[component]
fn SecurityRowStatic(label: &'static str, detail: &'static str) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between py-1">
            <div class="flex items-center gap-2">
                <span class="text-success" inner_html=ICON_SHIELD_CHECK></span>
                <span class="text-sm text-text-secondary">{label}</span>
            </div>
            <span class="text-sm font-medium">{detail}</span>
        </div>
    }
}

/// A security row whose status depends on a runtime boolean.
#[component]
fn SecurityRowDynamic(label: &'static str, detail: &'static str, verified: bool) -> impl IntoView {
    let (icon, color) = if verified {
        (ICON_SHIELD_CHECK, "text-success")
    } else {
        (ICON_ALERT_TRIANGLE, "text-warning")
    };

    view! {
        <div class="flex items-center justify-between py-1">
            <div class="flex items-center gap-2">
                <span class=color inner_html=icon></span>
                <span class="text-sm text-text-secondary">{label}</span>
            </div>
            <span class="text-sm font-medium">{detail}</span>
        </div>
    }
}
