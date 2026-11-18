//! Markdown rendering for server-provided text such as legal policies.
//!
//! The output is injected with `inner_html`, so it must be safe:
//! - raw HTML in the markdown is escaped and shown as text, never rendered;
//! - links keep only `http(s)`/`mailto` targets and open outside the app window;
//! - images are not loaded, only their alt text is kept.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use pulldown_cmark_escape::escape_href;

/// Converts markdown to HTML that is safe to render via `inner_html`.
pub fn markdown_to_safe_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let events = Parser::new_ext(markdown, options).filter_map(|event| match event {
        // Show raw HTML as literal text instead of rendering it.
        Event::Html(raw) | Event::InlineHtml(raw) => Some(Event::Text(raw)),
        Event::Start(Tag::Link { dest_url, .. }) => {
            Some(Event::InlineHtml(link_open(&dest_url).into()))
        }
        Event::End(TagEnd::Link) => Some(Event::InlineHtml("</a>".into())),
        // Drop the image itself; its alt text arrives as Text events in between.
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        other => Some(other),
    });

    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// Opening `<a>` tag with an escaped, scheme-checked href.
///
/// Unsafe or unknown targets (e.g. `javascript:`) become a link-less `<a>`.
fn link_open(dest_url: &str) -> String {
    if !is_allowed_url(dest_url) {
        return "<a>".to_owned();
    }
    let mut tag = String::from("<a href=\"");
    // escape_href percent-encodes quotes/angle brackets and turns `&` into `&amp;`,
    // so the value is safe inside a double-quoted attribute. Writing to a String
    // cannot fail.
    let _ = escape_href(&mut tag, dest_url.trim());
    tag.push_str("\" target=\"_blank\" rel=\"noopener noreferrer\">");
    tag
}

/// Only absolute web and mail links are allowed; anything else (relative
/// paths, `javascript:`, `data:`, ...) would navigate the app's own window.
fn is_allowed_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    ["https://", "http://", "mailto:"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headings_paragraphs_and_lists() {
        let html = markdown_to_safe_html("# Policy\n\nLine one\nline two\n\n- a\n- b");
        assert!(html.contains("<h1>Policy</h1>"));
        // Soft-wrapped lines join into one paragraph.
        assert!(html.contains("<p>Line one\nline two</p>"));
        assert!(html.contains("<li>a</li>"));
    }

    #[test]
    fn raw_html_is_escaped_not_rendered() {
        let html =
            markdown_to_safe_html("<script>alert(1)</script>\n\nhi <img src=x onerror=alert(1)>");
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn web_links_open_externally() {
        let html = markdown_to_safe_html("[site](https://example.com/a?b=1&c=2)");
        assert!(html.contains("href=\"https://example.com/a?b=1&amp;c=2\""));
        assert!(html.contains("target=\"_blank\""));
        assert!(html.contains("rel=\"noopener noreferrer\""));
        assert!(html.contains(">site</a>"));
    }

    #[test]
    fn mailto_links_are_allowed() {
        let html = markdown_to_safe_html("<mailto:help@example.com>");
        assert!(html.contains("href=\"mailto:help@example.com\""));
    }

    #[test]
    fn unsafe_and_relative_links_lose_their_href() {
        for md in [
            "[x](javascript:alert(1))",
            "[x](JavaScript:alert(1))",
            "[x](data:text/html,hi)",
            "[x](/settings)",
        ] {
            let html = markdown_to_safe_html(md);
            assert!(!html.contains("href"), "{md} -> {html}");
            assert!(html.contains("<a>x</a>"), "{md} -> {html}");
        }
    }

    #[test]
    fn link_href_cannot_break_out_of_attribute() {
        let html = markdown_to_safe_html("[x](<https://e.com/\"onmouseover=\"alert(1)>)");
        assert!(!html.contains("\"onmouseover"));
    }

    #[test]
    fn images_keep_only_alt_text() {
        let html = markdown_to_safe_html("![logo](https://tracker.example/p.png)");
        assert!(!html.contains("<img"));
        assert!(!html.contains("tracker.example"));
        assert!(html.contains("logo"));
    }
}
