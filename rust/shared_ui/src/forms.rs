use crate::{display::SuccessAlert, styles};
use leptos::prelude::*;

// ── SVG icons for password toggle ──────────────────────────────────

const EYE_ICON: &str = r#"<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>"#;

const EYE_OFF_ICON: &str = r#"<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 01-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg>"#;

/// Reusable password input with a visibility toggle button.
#[component]
pub fn PasswordInput(
    /// The `id` attribute for the input element.
    id: &'static str,
    /// The `name` attribute for native form submission.
    name: &'static str,
    /// Label text shown above the input.
    label: &'static str,
    /// Placeholder text.
    #[prop(default = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}")]
    placeholder: &'static str,
    /// Optional aria-label override; defaults to the label text.
    #[prop(optional, into)]
    aria_label: Option<String>,
    /// Optional input callback for parent-level validation.
    #[prop(optional)]
    on_input: Option<Callback<String>>,
) -> impl IntoView {
    let (visible, set_visible) = signal(false);
    let resolved_aria = aria_label.unwrap_or_else(|| label.to_string());

    view! {
        <div class="mb-5">
            <label class=styles::LABEL_CLASS for=id>{label}</label>
            <div class="relative">
                <input
                    type=move || if visible.get() { "text" } else { "password" }
                    id=id
                    name=name
                    aria-label=resolved_aria
                    placeholder=placeholder
                    class=styles::INPUT_CLASS
                    style="padding-right: 2.75rem;"
                    on:input=move |ev| {
                        if let Some(cb) = on_input.as_ref() {
                            cb.run(event_target_value(&ev));
                        }
                    }
                    required
                />
                <button
                    type="button"
                    tabindex=-1
                    class="absolute right-3 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary transition-colors"
                    aria-label="Toggle password visibility"
                    on:click=move |ev: leptos::ev::MouseEvent| {
                        ev.prevent_default();
                        set_visible.update(|v| *v = !*v);
                    }
                >
                    <span inner_html=move || if visible.get() { EYE_OFF_ICON } else { EYE_ICON }></span>
                </button>
            </div>
        </div>
    }
    .into_any()
}

/// Shared login form that works in both SSR (native form POST) and CSR (callback) modes.
///
/// - For SSR (website): pass `action` and `method`, use `name` attributes on inputs
/// - For CSR (leptos_ui): pass `on_submit` callback, use `on:input` signals
#[component]
pub fn LoginFormView(
    /// Form action URL for native HTML submission (SSR mode)
    #[prop(optional, into)]
    action: Option<String>,
    /// Title text above the form
    #[prop(default = "Welcome Back")]
    title: &'static str,
    /// Subtitle text
    #[prop(default = "Log in to manage your private backups")]
    subtitle: &'static str,
    /// Error message to display
    #[prop(optional, into)]
    error: MaybeProp<String>,
    /// Whether the form is in a loading state
    #[prop(optional)]
    loading: Signal<bool>,
    /// Button label
    #[prop(default = "Log In")]
    button_text: &'static str,
    /// Button label while loading
    #[prop(default = "Logging in...")]
    button_loading_text: &'static str,
    /// Children rendered inside the form (for extra fields or custom submit handling)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let form_method = action.as_ref().map(|_| "POST");

    view! {
        <form method=form_method action=action>
            <h2 class="text-3xl md:text-4xl font-display font-bold gradient-text mb-2">{title}</h2>
            <p class="text-text-secondary mb-6 md:mb-8">{subtitle}</p>

            <div class="mb-5">
                <label class=styles::LABEL_CLASS for="login-email">"Email"</label>
                <input
                    type="email"
                    id="login-email"
                    name="email"
                    aria-label="Login Email"
                    placeholder="john@example.com"
                    class=styles::INPUT_CLASS
                    required
                />
            </div>

            <PasswordInput id="login-password" name="password" label="Password" aria_label="Login Password".to_string() />

            {move || error.get().map(|e| view! {
                <div class="bg-error-tint border border-error-border text-error rounded-[var(--radius-base)] px-4 py-3 text-sm mb-5">{e}</div>
            })}

            {children.map(|c| c())}

            <button
                type="submit"
                class=styles::BTN_PRIMARY_CLASS
                disabled=move || loading.get()
            >
                {move || if loading.get() { button_loading_text } else { button_text }}
            </button>
        </form>
    }
}

/// Shared forgot password form that works in both SSR and CSR modes.
#[component]
pub fn ForgotPasswordFormView(
    /// Form action URL for native HTML submission (SSR mode)
    #[prop(optional, into)]
    action: Option<String>,
    /// Title text
    #[prop(default = "Forgot Password")]
    title: &'static str,
    /// Subtitle text
    #[prop(default = "Enter your email and we'll send you a reset link.")]
    subtitle: &'static str,
    /// Error message to display
    #[prop(optional, into)]
    error: MaybeProp<String>,
    /// Success message to display
    #[prop(optional, into)]
    success: MaybeProp<String>,
    /// Whether the form is in a loading state
    #[prop(optional)]
    loading: Signal<bool>,
    /// Button label
    #[prop(default = "Send Reset Link")]
    button_text: &'static str,
    /// Button label while loading
    #[prop(default = "Sending...")]
    button_loading_text: &'static str,
    /// Children rendered inside the form (e.g. "Back to login" link)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let form_method = action.as_ref().map(|_| "POST");

    view! {
        <form method=form_method action=action>
            <h2 class="text-3xl md:text-4xl font-display font-bold gradient-text mb-2">{title}</h2>
            <p class="text-text-secondary mb-6 md:mb-8">{subtitle}</p>

            {move || success.get().map(|msg| view! { <SuccessAlert message=msg /> })}

            {move || error.get().map(|e| view! {
                <div class="bg-error-tint border border-error-border text-error rounded-[var(--radius-base)] px-4 py-3 text-sm mb-5">{e}</div>
            })}

            <div class="mb-5">
                <label class=styles::LABEL_CLASS for="forgot-email">"Email"</label>
                <input
                    type="email"
                    id="forgot-email"
                    name="email"
                    aria-label="Forgot Email"
                    placeholder="john@example.com"
                    class=styles::INPUT_CLASS
                    required
                />
            </div>

            <button
                type="submit"
                class=styles::BTN_PRIMARY_CLASS
                disabled=move || loading.get()
            >
                {move || if loading.get() { button_loading_text } else { button_text }}
            </button>

            {children.map(|c| c())}
        </form>
    }
}

/// Shared signup form that works in both SSR and CSR modes.
#[component]
pub fn SignupFormView(
    /// Form action URL for native HTML submission (SSR mode)
    #[prop(optional, into)]
    action: Option<String>,
    /// Title text
    #[prop(default = "Create Account")]
    title: &'static str,
    /// Subtitle text
    #[prop(default = "Create your account to start private backup")]
    subtitle: &'static str,
    /// Error message to display
    #[prop(optional, into)]
    error: MaybeProp<String>,
    /// Whether the form is in a loading state (also changes button text to button_loading_text).
    #[prop(optional)]
    loading: Signal<bool>,
    /// Extra disabled condition that disables the button without changing its text.
    /// Use this for prerequisite gates (e.g. consent checkbox) so button text stays readable.
    #[prop(optional)]
    extra_disabled: Signal<bool>,
    /// Button label
    #[prop(default = "Sign Up")]
    button_text: &'static str,
    /// Button label while loading
    #[prop(default = "Creating...")]
    button_loading_text: &'static str,
    /// Children rendered inside the form
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let form_method = action.as_ref().map(|_| "POST");
    let (name_value, set_name_value) = signal(String::new());
    let (email_value, set_email_value) = signal(String::new());
    let (password_value, set_password_value) = signal(String::new());
    let name_invalid = move || name_value.get().trim().is_empty();
    let email_invalid = move || {
        let email = email_value.get();
        !email.is_empty() && !(email.contains('@') && email.contains('.'))
    };
    let password_invalid =
        move || !password_value.get().is_empty() && password_value.get().len() < 8;

    view! {
        <form method=form_method action=action>
            <h2 class="text-3xl md:text-4xl font-display font-bold gradient-text mb-2">{title}</h2>
            <p class="text-text-secondary mb-6 md:mb-8">{subtitle}</p>

            <div class="mb-5">
                <label class=styles::LABEL_CLASS for="signup-name">"Name"</label>
                <input
                    type="text"
                    id="signup-name"
                    name="name"
                    aria-label="Signup Name"
                    placeholder="John Doe"
                    class=styles::INPUT_CLASS
                    on:input=move |ev| set_name_value.set(event_target_value(&ev))
                    required
                />
                {move || name_invalid().then(|| view! {
                    <p class="text-xs text-text-secondary mt-1">"Name is required"</p>
                })}
            </div>

            <div class="mb-5">
                <label class=styles::LABEL_CLASS for="signup-email">"Email"</label>
                <input
                    type="email"
                    id="signup-email"
                    name="email"
                    aria-label="Signup Email"
                    placeholder="john@example.com"
                    class=styles::INPUT_CLASS
                    on:input=move |ev| set_email_value.set(event_target_value(&ev))
                    required
                />
                {move || email_invalid().then(|| view! {
                    <p class="text-xs text-error mt-1">"Enter a valid email address"</p>
                })}
            </div>

            <PasswordInput
                id="signup-password"
                name="password"
                label="Password"
                aria_label="Signup Password".to_string()
                on_input=Callback::new(move |value| set_password_value.set(value))
            />
            {move || password_invalid().then(|| view! {
                <p class="text-xs text-error -mt-3 mb-5">"Use at least 8 characters"</p>
            })}

            {move || error.get().map(|e| view! {
                <div class="bg-error-tint border border-error-border text-error rounded-[var(--radius-base)] px-4 py-3 text-sm mb-5">{e}</div>
            })}

            {children.map(|c| c())}

            <button
                type="submit"
                class=styles::BTN_PRIMARY_CLASS
                disabled=move || loading.get() || extra_disabled.get()
            >
                {move || if loading.get() { button_loading_text } else { button_text }}
            </button>
        </form>
    }
}
