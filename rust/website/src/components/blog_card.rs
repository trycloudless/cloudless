use api_types::blog::BlogPostSummary;
use leptos::prelude::*;

#[component]
pub fn BlogCard(post: BlogPostSummary) -> impl IntoView {
    let href = format!("/blog/{}", post.slug);
    let date = post.created_at.format("%B %d, %Y").to_string();
    let title = post.title;
    let summary = post.summary;

    view! {
        <a href={href} class="block group">
            <article class="glass bg-surface border border-border rounded-2xl p-6 hover:border-primary-glow transition-all duration-300 group-hover:-translate-y-1">
                <time class="text-xs text-text-secondary uppercase tracking-wider">{date}</time>
                <h3 class="text-xl font-semibold text-text-primary mt-2 mb-3 group-hover:text-primary-light transition-colors">
                    {title}
                </h3>
                <p class="text-text-secondary text-sm leading-relaxed">
                    {summary}
                </p>
                <span class="inline-block mt-4 text-sm text-primary-light font-medium group-hover:translate-x-1 transition-transform">
                    "Read more →"
                </span>
            </article>
        </a>
    }
}
