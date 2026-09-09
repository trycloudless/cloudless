/// Theme management utilities for switching between light and dark modes.
///
/// Requires the `theme` feature flag (enabled automatically for WASM/CSR targets).
/// In SSR context, theme is set via inline `<script>` in the HTML `<head>`.

/// Get the current theme from the `data-theme` attribute on `<html>`.
/// Returns `"dark"` or `"light"`.
pub fn get_current_theme() -> String {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .expect("document should exist");
    document
        .document_element()
        .and_then(|el| el.get_attribute("data-theme"))
        .unwrap_or_else(|| "dark".to_string())
}

/// Get the user's stored theme preference from localStorage.
/// Returns `"auto"`, `"light"`, or `"dark"`. Defaults to `"auto"` when unset.
pub fn get_theme_preference() -> String {
    let window = web_sys::window().expect("window should exist");
    window
        .local_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item("theme").ok().flatten())
        .unwrap_or_else(|| "auto".to_string())
}

/// Set the theme preference to `"auto"`, `"light"`, or `"dark"`.
///
/// When `"auto"`, detects from `prefers-color-scheme` media query.
/// Updates the `data-theme` attribute on `<html>` and persists to localStorage.
pub fn set_theme(theme: &str) {
    let window = web_sys::window().expect("window should exist");
    let document = window.document().expect("document should exist");

    let effective = if theme == "auto" {
        let prefers_dark = window
            .match_media("(prefers-color-scheme:dark)")
            .ok()
            .flatten()
            .map(|mql| mql.matches())
            .unwrap_or(true);
        if prefers_dark { "dark" } else { "light" }
    } else {
        theme
    };

    if let Some(el) = document.document_element() {
        let _ = el.set_attribute("data-theme", effective);
    }

    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item("theme", theme);
    }
}

/// Toggle between light and dark themes.
/// Returns the new theme name.
pub fn toggle_theme() -> String {
    let current = get_current_theme();
    let new_theme = if current == "dark" { "light" } else { "dark" };
    set_theme(new_theme);
    new_theme.to_string()
}
