use ammonia::Builder;
use pulldown_cmark::{Options, Parser, html};
use std::collections::HashSet;

/// Converts markdown to sanitized HTML safe for rendering via `inner_html`.
///
/// Supports tables, strikethrough, and task lists. Inline HTML in the markdown
/// is preserved if it uses allowed tags/attributes (standard blog elements
/// plus `img` with `src`/`alt`/`class`).
pub fn markdown_to_safe_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    let mut tag_attributes = Builder::default().clone_tag_attributes();

    // Ensure img tags keep src, alt, class, and style attributes
    let img_attrs: HashSet<&str> = ["src", "alt", "class", "style", "width", "height"]
        .iter()
        .copied()
        .collect();
    tag_attributes.insert("img", img_attrs);

    Builder::default()
        .tag_attributes(tag_attributes)
        .clean(&html_output)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_markdown() {
        let result = markdown_to_safe_html("# Hello\n\nWorld");
        assert!(result.contains("<h1>Hello</h1>"));
        assert!(result.contains("<p>World</p>"));
    }

    #[test]
    fn test_image_preserved() {
        let result = markdown_to_safe_html("![alt text](/media/test.png)");
        assert!(result.contains("<img"));
        assert!(result.contains("src=\"/media/test.png\""));
        assert!(result.contains("alt=\"alt text\""));
    }

    #[test]
    fn test_inline_html_img() {
        let result = markdown_to_safe_html(
            "<img src=\"/media/test.png\" alt=\"photo\" class=\"rounded\" />",
        );
        assert!(result.contains("src=\"/media/test.png\""));
        assert!(result.contains("class=\"rounded\""));
    }

    #[test]
    fn test_script_stripped() {
        let result = markdown_to_safe_html("<script>alert('xss')</script>");
        assert!(!result.contains("<script>"));
    }

    #[test]
    fn test_table_support() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let result = markdown_to_safe_html(md);
        assert!(result.contains("<table>"));
    }
}
