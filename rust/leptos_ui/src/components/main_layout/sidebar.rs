use crate::state::*;
use leptos::prelude::*;

// ── SVG Icon Constants ─────────────────────────────────────────

const ICON_DASHBOARD: &str = r#"<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/></svg>"#;

const ICON_FILES: &str = r#"<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>"#;

const ICON_BACKUP: &str = r#"<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 16 12 12 8 16"/><line x1="12" y1="12" x2="12" y2="21"/><path d="M20.39 18.39A5 5 0 0018 9h-1.26A8 8 0 103 16.3"/></svg>"#;

const ICON_SETTINGS: &str = r#"<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83 0 2 2 0 010-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 010-2.83 2 2 0 012.83 0l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 0 2 2 0 010 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>"#;

const ICON_LOCK: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>"#;

const ICON_LOGOUT: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>"#;

struct NavItem {
    label: &'static str,
    icon: &'static str,
    view: MainView,
}

const NAV_ITEMS: &[NavItem] = &[
    NavItem {
        label: "Dashboard",
        icon: ICON_DASHBOARD,
        view: MainView::Dashboard,
    },
    NavItem {
        label: "Files",
        icon: ICON_FILES,
        view: MainView::Files,
    },
    NavItem {
        label: "Backup",
        icon: ICON_BACKUP,
        view: MainView::Backup,
    },
    NavItem {
        label: "Settings",
        icon: ICON_SETTINGS,
        view: MainView::Settings,
    },
];

#[component]
pub fn Sidebar(active_view: MainView) -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_auth_mode) = use_context::<AuthModeSignal>().unwrap();
    let (session, set_session) = use_context::<SessionSignal>().unwrap();
    let (backup_progress, _) = use_context::<BackupProgressSignal>().unwrap();
    let (restore_progress, _) = use_context::<RestoreProgressSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let on_lock = move |_: leptos::ev::MouseEvent| {
        is_unlocked.set(false);
        set_auth_mode.set(AuthMode::UnlockEncryption);
        set_phase.set(AppPhase::Auth);
    };

    let on_logout = move |_: leptos::ev::MouseEvent| {
        set_session.set(SessionInfo::default());
        set_auth_mode.set(AuthMode::Login);
        set_phase.set(AppPhase::Auth);
    };

    view! {
        // Desktop sidebar — hidden on mobile
        <aside class="hidden md:flex w-60 bg-sidebar border-r border-border flex-col h-screen">
            <div class="px-6 py-5 border-b border-border">
                <h1 class="text-xl font-display font-bold gradient-text">"CloudLess"</h1>
                <p class="text-xs text-text-secondary mt-1">"Private backup you control"</p>
            </div>

            <nav class="flex-1 px-3 py-4 space-y-1">
                {NAV_ITEMS.iter().map(|item| {
                    let is_active = active_view == item.view
                        || (item.view == MainView::Backup && active_view == MainView::BackupReports);
                    let target_view = item.view;
                    let label = item.label;
                    let icon = item.icon;

                    view! {
                        <button
                            class=move || if is_active {
                                "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg bg-primary-tint text-primary-light font-medium text-sm transition-all"
                            } else {
                                "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-text-secondary hover:bg-white/5 hover:text-text-primary text-sm transition-all"
                            }
                            on:click=move |_| set_phase.set(AppPhase::Main(target_view))
                        >
                            <span class="flex-shrink-0" inner_html=icon></span>
                            <span>{label}</span>
                        </button>
                    }
                }).collect_view()}
            </nav>

            <div class="px-4 py-4 border-t border-border space-y-3">
                {move || {
                    let bp = backup_progress.get();
                    let rp = restore_progress.get();
                    let signed_in = session.get().user_id.is_some();
                    let (dot_class, label_class, label) = if bp.is_running {
                        ("bg-accent animate-pulse", "text-accent", "Backup running")
                    } else if rp.is_running {
                        ("bg-accent animate-pulse", "text-accent", "Restore running")
                    } else if !signed_in {
                        ("bg-warning", "text-warning", "Waiting for sign-in")
                    } else if bp.phase.starts_with("Failed") || rp.phase.starts_with("Failed") {
                        ("bg-warning", "text-warning", "Action needed")
                    } else {
                        ("bg-text-secondary", "text-text-secondary", "Ready")
                    };
                        view! {
                            <div class="flex items-center gap-2 text-sm">
                                <div class={format!("w-2 h-2 rounded-full {}", dot_class)}></div>
                                <span class={format!("{} text-xs", label_class)}>{label}</span>
                            </div>
                        }.into_any()
                }}

                <div class="flex items-center gap-2">
                    <button
                        class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-text-secondary hover:bg-white/5 hover:text-text-primary text-xs transition-all"
                        title="Lock"
                        aria-label="Lock application"
                        on:click=on_lock
                    >
                        <span class="flex-shrink-0" inner_html=ICON_LOCK></span>
                        <span>"Lock"</span>
                    </button>
                    <button
                        class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-text-secondary hover:bg-error-tint hover:text-error text-xs transition-all"
                        title="Sign out"
                        aria-label="Sign out"
                        on:click=on_logout
                    >
                        <span class="flex-shrink-0" inner_html=ICON_LOGOUT></span>
                        <span>"Sign Out"</span>
                    </button>
                </div>
            </div>
        </aside>

        // Mobile top bar — session controls (hidden on desktop)
        <div class="md:hidden flex items-center justify-between px-4 py-2 bg-sidebar border-b border-border">
            <div>
                <span class="text-sm font-display font-bold gradient-text">"CloudLess"</span>
            </div>
            <div class="flex items-center gap-1">
                <button
                    class="flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-text-secondary hover:bg-white/5 hover:text-text-primary text-xs transition-all"
                    aria-label="Lock application"
                    on:click=on_lock
                >
                    <span class="flex-shrink-0" inner_html=ICON_LOCK></span>
                    <span>"Lock"</span>
                </button>
                <button
                    class="flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-text-secondary hover:bg-error-tint hover:text-error text-xs transition-all"
                    aria-label="Sign out"
                    on:click=on_logout
                >
                    <span class="flex-shrink-0" inner_html=ICON_LOGOUT></span>
                    <span>"Sign Out"</span>
                </button>
            </div>
        </div>

        // Mobile bottom nav — hidden on desktop
        // env(safe-area-inset-bottom) works on iOS; Android WebView returns 0 so we use a fallback
        <nav class="fixed bottom-0 left-0 right-0 md:hidden bg-sidebar border-t border-border z-50"
             style="padding-bottom: env(safe-area-inset-bottom, 16px)">
            <div class="flex items-center justify-around h-14">
                {NAV_ITEMS.iter().map(|item| {
                    let is_active = active_view == item.view
                        || (item.view == MainView::Backup && active_view == MainView::BackupReports);
                    let target_view = item.view;
                    let label = item.label;
                    let icon = item.icon;

                    view! {
                        <button
                            aria-label=label
                            class=move || if is_active {
                                "flex flex-col items-center justify-center gap-0.5 px-3 py-1 text-primary-light"
                            } else {
                                "flex flex-col items-center justify-center gap-0.5 px-3 py-1 text-text-secondary"
                            }
                            on:click=move |_| set_phase.set(AppPhase::Main(target_view))
                        >
                            <span class="flex-shrink-0" inner_html=icon></span>
                            <span class="text-xs font-medium">{label}</span>
                        </button>
                    }
                }).collect_view()}
            </div>
        </nav>
    }
}
