use axum::response::Html;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::components::layout::Layout;

#[component]
fn NotFoundPage() -> impl IntoView {
    view! {
        <div class="max-w-2xl mx-auto px-6 py-24 text-center">
            <h1 class="text-6xl font-display font-bold gradient-text mb-4">"404"</h1>
            <p class="text-xl text-text-secondary mb-8">
                "The page you're looking for doesn't exist."
            </p>
            <a
                href="/"
                class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all inline-block"
            >
                "Go Home"
            </a>
        </div>
    }
}

pub fn render_404() -> String {
    view! {
        <Layout title="Not Found - CloudLess".to_string()>
            <NotFoundPage />
        </Layout>
    }
    .to_html()
}

pub async fn not_found_handler() -> Html<String> {
    Html(render_404())
}
