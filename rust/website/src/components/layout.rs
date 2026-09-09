use crate::auth::AuthState;
use leptos::prelude::*;
use leptos::tachys::html::attribute::custom::CustomAttribute;

use super::{footer::Footer, nav::Nav};

/// Creates a `<meta property="..." content="...">` element using the tachys
/// custom attribute API, since `property` is not a standard HTML attribute
/// recognised by the Leptos view macro.
fn meta_property(property: &'static str, content: String) -> impl IntoView {
    leptos::html::meta()
        .attr("property", property)
        .attr("content", content)
}

#[component]
pub fn Layout(
    title: String,
    #[prop(optional)] auth: AuthState,
    /// Empty string = no description meta tag rendered.
    #[prop(default = String::new())]
    description: String,
    /// Empty string = falls back to the default og-image.
    #[prop(default = String::new())]
    og_image: String,
    /// When true, includes marked.js and editor.js scripts for the markdown editor.
    #[prop(optional)]
    editor_mode: bool,
    children: Children,
) -> impl IntoView {
    let base =
        std::env::var("SITE_BASE_URL").unwrap_or_else(|_| "https://trycloudless.io".to_string());
    let base = base.trim_end_matches('/').to_string();
    let og_image_url = if og_image.is_empty() {
        format!("{base}/static/og-image.png")
    } else if og_image.starts_with("http") {
        og_image
    } else {
        format!("{base}{og_image}")
    };
    let title_clone = title.clone();
    let has_desc = !description.is_empty();
    let desc_clone = description.clone();
    let og_image_clone = og_image_url.clone();
    view! {
        <html lang="en" data-theme="dark">
            <head>
                <meta charset="UTF-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1.0" />
                <title>{title.clone()}</title>
                {has_desc.then(|| view! {
                    <meta name="description" content=description.clone() />
                })}
                {meta_property("og:title", title_clone)}
                {has_desc.then(|| meta_property("og:description", desc_clone.clone()))}
                {meta_property("og:type", "website".to_string())}
                {meta_property("og:image", og_image_clone.clone())}
                <meta name="twitter:card" content="summary_large_image" />
                <meta name="twitter:title" content=title />
                {has_desc.then(|| view! {
                    <meta name="twitter:description" content=desc_clone />
                })}
                <meta name="twitter:image" content=og_image_clone />
                <link rel="icon" href="/static/favicon.ico" sizes="any" />
                <link rel="icon" href="/static/favicon-32x32.png" type="image/png" sizes="32x32" />
                <link rel="apple-touch-icon" href="/static/apple-touch-icon.png" />
                <meta name="theme-color" content="#1e293b" />
                <link rel="stylesheet" href="/static/output.css" />
                <link rel="preconnect" href="https://fonts.googleapis.com" />
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
                <link
                    href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Outfit:wght@600;700&display=swap"
                    rel="stylesheet"
                />
                <script>
                    // Theme: check localStorage, fallback to system preference
                    "(function(){var t=localStorage.getItem('theme');if(!t)t=matchMedia('(prefers-color-scheme:dark)').matches?'dark':'light';document.documentElement.setAttribute('data-theme',t)})();"
                </script>
                <script>
                    // Platform detection
                    "(function(){var ua=navigator.userAgent;var p=/iPad|iPhone|iPod|Mac/.test(ua)?'ios':'android';document.documentElement.setAttribute('data-platform',p)})();"
                </script>
                {if editor_mode {
                    view! {
                        <script
                            src="https://cdn.jsdelivr.net/npm/marked@15.0.7/marked.min.js"
                            integrity="sha384-H+hy9ULve6xfxRkWIh/YOtvDdpXgV2fmAGQkIDTxIgZwNoaoBal14Di2YTMR6MzR"
                            crossorigin="anonymous"
                            defer="true"
                        ></script>
                        <script
                            src="https://cdn.jsdelivr.net/npm/dompurify@3.2.5/dist/purify.min.js"
                            integrity="sha384-qSFej5dZNviyoPgYJ5+Xk4bEbX8AYddxAHPuzs1aSgRiXxJ3qmyWNaPsRkpv/+x5"
                            crossorigin="anonymous"
                            defer="true"
                        ></script>
                        <script src="/static/editor.js" defer="true"></script>
                    }.into_any()
                } else {
                    view! { <span /> }.into_any()
                }}
            </head>
            <body class="min-h-screen bg-bg text-text-primary font-sans overflow-x-hidden">
                <Nav auth=auth />
                <main class="animate-fade-in">
                    {children()}
                </main>
                <Footer />
            </body>
        </html>
    }
}
