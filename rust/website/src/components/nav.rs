use crate::auth::AuthState;
use leptos::prelude::*;

#[component]
pub fn Nav(#[prop(optional)] auth: AuthState) -> impl IntoView {
    let is_admin = auth.is_admin();
    let is_authenticated = auth.is_authenticated();
    let user_name = auth.name().to_string();
    let initial = user_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "U".to_string());

    view! {
        <nav class="glass bg-surface-alpha border-b border-border sticky top-0 z-50">
            <div class="max-w-6xl mx-auto px-6 py-4 flex items-center justify-between">
                <a href="/" class="text-2xl font-display font-bold gradient-text">
                    "CloudLess"
                </a>

                // Mobile menu button
                <button
                    class="md:hidden text-text-secondary hover:text-text-primary"
                    onclick="document.getElementById('mobile-menu').classList.toggle('hidden')"
                >
                    <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
                    </svg>
                </button>

                // Desktop nav
                <div class="hidden md:flex items-center gap-8">
                    <a href="/" class="text-text-secondary hover:text-text-primary transition-colors">"Home"</a>
                    <a href="/pricing" class="text-text-secondary hover:text-text-primary transition-colors">"Pricing"</a>
                    <a href="/download" class="text-text-secondary hover:text-text-primary transition-colors">"Download"</a>
                    <a href="/blog" class="text-text-secondary hover:text-text-primary transition-colors">"Blog"</a>
                    {if is_admin {
                        view! {
                            <a href="/admin" class="text-text-secondary hover:text-text-primary transition-colors">"Admin"</a>
                        }.into_any()
                    } else {
                        view! { <span /> }.into_any()
                    }}
                    {if is_authenticated {
                        let initial_clone = initial.clone();
                        view! {
                            <div class="relative">
                                <button
                                    class="w-9 h-9 rounded-full bg-primary-tint border border-primary-ring flex items-center justify-center text-primary-light font-semibold text-sm cursor-pointer hover:bg-primary-glow transition-all"
                                    title=user_name.clone()
                                    onclick="const menu = document.getElementById('profile-menu'); menu.classList.toggle('hidden'); event.stopPropagation();"
                                >
                                    {initial_clone}
                                </button>
                                <div id="profile-menu" class="hidden absolute right-0 mt-2 w-40 bg-surface border border-border rounded-xl shadow-xl overflow-hidden z-50">
                                    <div class="px-4 py-2 border-b border-border">
                                        <p class="text-sm text-text-primary font-medium truncate">{user_name.clone()}</p>
                                    </div>
                                    <form method="POST" action="/logout">
                                        <button type="submit" class="w-full text-left px-4 py-2 text-sm text-text-secondary hover:bg-primary-tint hover:text-text-primary transition-colors">"Logout"</button>
                                    </form>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <a
                                href="/login"
                                class="btn-gradient text-white px-5 py-2 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                            >
                                "Login"
                            </a>
                        }.into_any()
                    }}
                </div>
            </div>

            // Close profile dropdown when clicking outside
            <script>"document.addEventListener('click', function() { var m = document.getElementById('profile-menu'); if (m) m.classList.add('hidden'); });"</script>

            // Mobile nav
            <div id="mobile-menu" class="hidden md:hidden border-t border-border">
                <div class="px-6 py-4 flex flex-col gap-4">
                    <a href="/" class="text-text-secondary hover:text-text-primary transition-colors">"Home"</a>
                    <a href="/pricing" class="text-text-secondary hover:text-text-primary transition-colors">"Pricing"</a>
                    <a href="/download" class="text-text-secondary hover:text-text-primary transition-colors">"Download"</a>
                    <a href="/blog" class="text-text-secondary hover:text-text-primary transition-colors">"Blog"</a>
                    {if is_admin {
                        view! {
                            <a href="/admin" class="text-text-secondary hover:text-text-primary transition-colors">"Admin"</a>
                        }.into_any()
                    } else {
                        view! { <span /> }.into_any()
                    }}
                    {if is_authenticated {
                        view! {
                            <div class="flex items-center justify-between">
                                <div class="flex items-center gap-2 text-text-secondary">
                                    <div class="w-8 h-8 rounded-full bg-primary-tint border border-primary-ring flex items-center justify-center text-primary-light font-semibold text-xs">
                                        {initial.clone()}
                                    </div>
                                    <span>{user_name.clone()}</span>
                                </div>
                                <form method="POST" action="/logout">
                                    <button type="submit" class="text-sm text-text-secondary hover:text-text-primary transition-colors">"Logout"</button>
                                </form>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <a
                                href="/login"
                                class="btn-gradient text-white px-5 py-2 rounded-xl font-semibold text-center shadow-lg shadow-primary-tint"
                            >
                                "Login"
                            </a>
                        }.into_any()
                    }}
                </div>
            </div>
        </nav>
    }
}
