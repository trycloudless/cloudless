use crate::markdown::markdown_to_safe_html;
use leptos::prelude::*;

/// Scrollable box that renders a policy's markdown content.
///
/// The policy title is shown by the caller, so a leading `# Title` heading in
/// the markdown is hidden to avoid showing it twice. Content goes through
/// [`markdown_to_safe_html`], which escapes raw HTML and restricts links.
#[component]
pub fn PolicyContent(
    /// Markdown source of the policy.
    #[prop(into)]
    content: String,
    /// Extra classes for the outer box (e.g. margins).
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let html = markdown_to_safe_html(&content);
    let classes = format!(
        "max-h-64 overflow-y-auto bg-bg rounded-xl p-4 text-sm text-text-secondary border border-border-alpha \
         [&_h1]:hidden \
         [&_h2]:text-base [&_h2]:font-semibold [&_h2]:text-text-primary [&_h2]:mt-4 [&_h2]:mb-2 \
         [&_h3]:text-sm [&_h3]:font-semibold [&_h3]:text-text-primary [&_h3]:mt-3 [&_h3]:mb-1 \
         [&_p]:leading-relaxed [&_p]:mb-3 \
         [&_ul]:list-disc [&_ul]:pl-5 [&_ul]:mb-3 [&_ul]:space-y-1 \
         [&_ol]:list-decimal [&_ol]:pl-5 [&_ol]:mb-3 [&_ol]:space-y-1 \
         [&_a]:text-primary-light [&_a]:underline \
         [&_strong]:text-text-primary [&_strong]:font-semibold \
         [&_code]:bg-surface [&_code]:px-1 [&_code]:rounded \
         [&_table]:w-full [&_table]:mb-3 [&_th]:text-left [&_th]:text-text-primary [&_th]:pr-3 [&_td]:pr-3 [&_td]:align-top \
         [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 {class}"
    );

    view! { <div class=classes inner_html=html /> }
}
