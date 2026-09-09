use leptos::prelude::*;

#[component]
pub fn Footer() -> impl IntoView {
    view! {
        <footer class="border-t border-border mt-20">
            <div class="max-w-6xl mx-auto px-6 py-12">
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-8">
                    // Brand
                    <div class="sm:col-span-2 lg:col-span-1">
                        <h3 class="text-xl font-display font-bold gradient-text mb-3">"CloudLess"</h3>
                        <p class="text-text-secondary text-sm">
                            "Secure, encrypted cloud backup for your files. Zero-knowledge architecture means only you can access your data."
                        </p>
                        <p class="text-text-secondary text-sm mt-3">
                            "Need help? "
                            <a href="mailto:support@trycloudless.io" class="text-primary-light hover:underline">
                                "Contact support"
                            </a>
                        </p>
                    </div>

                    // Product
                    <div>
                        <h4 class="text-sm font-semibold text-text-primary mb-3 uppercase tracking-wider">"Product"</h4>
                        <ul class="space-y-2 text-sm">
                            <li><a href="/" class="text-text-secondary hover:text-text-primary transition-colors">"Home"</a></li>
                            <li><a href="/pricing" class="text-text-secondary hover:text-text-primary transition-colors">"Pricing"</a></li>
                            <li><a href="/download" class="text-text-secondary hover:text-text-primary transition-colors">"Download"</a></li>
                            <li><a href="/blog" class="text-text-secondary hover:text-text-primary transition-colors">"Blog"</a></li>
                        </ul>
                    </div>

                    // Legal
                    <div>
                        <h4 class="text-sm font-semibold text-text-primary mb-3 uppercase tracking-wider">"Legal"</h4>
                        <ul class="space-y-2 text-sm">
                            <li><a href="/legal/privacy-policy" class="text-text-secondary hover:text-text-primary transition-colors">"Privacy Policy"</a></li>
                            <li><a href="/legal/terms-of-service" class="text-text-secondary hover:text-text-primary transition-colors">"Terms of Service"</a></li>
                            <li><a href="/legal/cookie-policy" class="text-text-secondary hover:text-text-primary transition-colors">"Cookie Policy"</a></li>
                            <li><a href="/legal/refund-and-cancellation" class="text-text-secondary hover:text-text-primary transition-colors">"Refund and Cancellation"</a></li>
                            <li><a href="/legal/data-retention-and-deletion" class="text-text-secondary hover:text-text-primary transition-colors">"Data Retention and Deletion"</a></li>
                            <li><a href="/legal/security-and-data-handling" class="text-text-secondary hover:text-text-primary transition-colors">"Security and Data Handling"</a></li>
                            <li><a href="/legal/subprocessors" class="text-text-secondary hover:text-text-primary transition-colors">"Subprocessors"</a></li>
                            <li><a href="/legal/acceptable-use" class="text-text-secondary hover:text-text-primary transition-colors">"Acceptable Use"</a></li>
                        </ul>
                    </div>

                    // Resources
                    <div>
                        <h4 class="text-sm font-semibold text-text-primary mb-3 uppercase tracking-wider">"Resources"</h4>
                        <ul class="space-y-2 text-sm">
                            <li><a href="/blog/backup-vs-sync" class="text-text-secondary hover:text-text-primary transition-colors">"Backup vs Sync"</a></li>
                            <li><a href="/blog/zero-knowledge-backup" class="text-text-secondary hover:text-text-primary transition-colors">"Zero-Knowledge Backup"</a></li>
                            <li><a href="/blog/ransomware-recovery" class="text-text-secondary hover:text-text-primary transition-colors">"Ransomware Recovery"</a></li>
                        </ul>
                    </div>
                </div>

                <div class="border-t border-border mt-8 pt-8 text-center text-sm text-text-secondary">
                    <p>"© 2026 CloudLess. All rights reserved."</p>
                </div>
            </div>
        </footer>
    }
}
