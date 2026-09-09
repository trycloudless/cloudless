use leptos::prelude::*;

#[component]
pub fn StatusBadge(status: String) -> impl IntoView {
    let (class, icon) = match status.as_str() {
        "completed" => (
            "bg-success-tint text-success",
            r#"<svg class="w-3 h-3 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>"#,
        ),
        "failed" => (
            "bg-error-tint text-error",
            r#"<svg class="w-3 h-3 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>"#,
        ),
        _ => (
            "bg-warning-tint text-warning",
            r#"<svg class="w-3 h-3 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>"#,
        ),
    };

    view! {
        <span class={format!("inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium {}", class)}>
            <span inner_html=icon></span>
            {status}
        </span>
    }
}

#[component]
pub fn StatRow(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex justify-between py-2 border-b border-border">
            <span class="text-text-secondary text-sm">{label}</span>
            <span class="text-sm">{value}</span>
        </div>
    }
}

#[component]
pub fn EmptyState(
    message: &'static str,
    #[prop(optional)] icon: Option<&'static str>,
    #[prop(optional)] action_label: Option<&'static str>,
    #[prop(optional)] on_action: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="glass bg-surface border border-border rounded-2xl p-6 md:p-8 text-center">
            {icon.map(|svg| view! {
                <div class="flex justify-center mb-3">
                    <div class="w-12 h-12 rounded-xl bg-primary-tint flex items-center justify-center" inner_html=svg></div>
                </div>
            })}
            <p class="text-text-secondary">{message}</p>
            {action_label.map(|label| {
                let cb = on_action.clone();
                view! {
                    <button
                        class="mt-4 px-5 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                        on:click=move |_| { if let Some(ref cb) = cb { cb.run(()); } }
                    >
                        {label}
                    </button>
                }
            })}
        </div>
    }
}

#[component]
pub fn ErrorAlert(message: String) -> impl IntoView {
    view! {
        <div class="bg-error-tint border border-error-border text-error rounded-[var(--radius-base)] px-4 py-3 text-sm mb-5">
            {message}
        </div>
    }
}

#[component]
pub fn SuccessAlert(message: String) -> impl IntoView {
    view! {
        <div class="bg-accent-tint border border-accent-light text-accent-light rounded-[var(--radius-base)] px-4 py-3 text-sm mb-5">
            {message}
        </div>
    }
}
