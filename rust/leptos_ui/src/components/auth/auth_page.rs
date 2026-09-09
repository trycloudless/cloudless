use crate::state::*;
use leptos::prelude::*;

use super::forgot_password_form::ForgotPasswordForm;
use super::login_form::LoginForm;
use super::signup_form::SignupForm;
use super::unlock_form::UnlockForm;

#[component]
pub fn AuthPage() -> impl IntoView {
    let (auth_mode, set_auth_mode) = use_context::<AuthModeSignal>().unwrap();

    view! {
        <div class="flex items-center justify-center min-h-[100dvh] p-4 md:p-8">
            <div class="glass bg-surface border border-border rounded-3xl p-6 md:p-10 w-full max-w-md shadow-2xl">
                {move || match auth_mode.get() {
                    AuthMode::Login => view! { <LoginForm /> }.into_any(),
                    AuthMode::Signup => view! { <SignupForm /> }.into_any(),
                    AuthMode::ForgotPassword => view! { <ForgotPasswordForm /> }.into_any(),
                    AuthMode::UnlockEncryption => view! { <UnlockForm /> }.into_any(),
                }}

                <div class="mt-6 flex justify-center gap-2 text-sm">
                    {move || match auth_mode.get() {
                        AuthMode::Login => view! {
                            <span class="text-text-secondary">"Don't have an account?"</span>
                            <button
                                class="text-primary-light font-medium hover:text-white transition-colors"
                                on:click=move |_| set_auth_mode.set(AuthMode::Signup)
                            >
                                "Sign Up"
                            </button>
                        }.into_any(),
                        AuthMode::Signup => view! {
                            <span class="text-text-secondary">"Already have an account?"</span>
                            <button
                                class="text-primary-light font-medium hover:text-white transition-colors"
                                on:click=move |_| set_auth_mode.set(AuthMode::Login)
                            >
                                "Log In"
                            </button>
                        }.into_any(),
                        AuthMode::ForgotPassword => view! {
                            <button
                                class="text-primary-light font-medium hover:text-white transition-colors"
                                on:click=move |_| set_auth_mode.set(AuthMode::Login)
                            >
                                "Back to Sign In"
                            </button>
                        }.into_any(),
                        AuthMode::UnlockEncryption => view! {
                            <button
                                class="text-text-secondary text-xs hover:text-text-primary transition-colors"
                                on:click=move |_| set_auth_mode.set(AuthMode::Login)
                            >
                                "Sign in with a different account"
                            </button>
                        }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}
