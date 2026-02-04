//! SIMD-accelerated HTML to Markdown converter.
//!
//! This crate provides fast HTML to Markdown conversion using portable SIMD
//! for accelerated byte scanning.
//!
//! # Example
//!
//! ```
//! use simd_html_to_md::html_to_md;
//!
//! let html = "<h1>Hello</h1><p>This is <strong>bold</strong> text.</p>";
//! let markdown = html_to_md(html);
//! assert!(markdown.contains("# Hello"));
//! assert!(markdown.contains("**bold**"));
//! ```
//!
//! # Features
//!
//! - SIMD-accelerated scanning for `<`, `>`, and `&` characters
//! - Full CommonMark parity
//! - GFM table support
//! - Best-effort error recovery for malformed HTML
//! - Zero external dependencies (uses `std::simd`)

#![feature(portable_simd)]

mod converter;
mod emitter;
mod entities;
mod simd;
mod tokenizer;

pub use converter::Options;

/// Convert HTML to Markdown using default options.
///
/// # Example
///
/// ```
/// use simd_html_to_md::html_to_md;
///
/// let html = "<p>Hello <em>world</em>!</p>";
/// let md = html_to_md(html);
/// assert_eq!(md, "Hello *world*\\!\n");
/// ```
pub fn html_to_md(html: &str) -> String {
    converter::convert(html, &Options::default())
}

/// Convert HTML to Markdown with custom options.
///
/// # Example
///
/// ```
/// use simd_html_to_md::{html_to_md_with_options, Options};
///
/// let options = Options {
///     bullet_char: '*',
///     ..Default::default()
/// };
///
/// let html = "<ul><li>Item</li></ul>";
/// let md = html_to_md_with_options(html, options);
/// assert!(md.contains("* Item"));
/// ```
pub fn html_to_md_with_options(html: &str, options: Options) -> String {
    converter::convert(html, &options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_conversion() {
        let html = "<h1>Title</h1><p>Paragraph</p>";
        let md = html_to_md(html);
        assert!(md.contains("# Title"));
        assert!(md.contains("Paragraph"));
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(html_to_md(""), "");
    }

    #[test]
    fn test_plain_text() {
        assert_eq!(html_to_md("Just text"), "Just text\n");
    }

    #[test]
    fn test_custom_bullet() {
        let options = Options {
            bullet_char: '*',
            ..Default::default()
        };
        let html = "<ul><li>Item</li></ul>";
        let md = html_to_md_with_options(html, options);
        assert!(md.contains("* Item"));
    }

    #[test]
    fn test_all_heading_levels() {
        assert!(html_to_md("<h1>H1</h1>").contains("# H1"));
        assert!(html_to_md("<h2>H2</h2>").contains("## H2"));
        assert!(html_to_md("<h3>H3</h3>").contains("### H3"));
        assert!(html_to_md("<h4>H4</h4>").contains("#### H4"));
        assert!(html_to_md("<h5>H5</h5>").contains("##### H5"));
        assert!(html_to_md("<h6>H6</h6>").contains("###### H6"));
    }

    #[test]
    fn test_links_and_images() {
        let html = r#"<a href="url">text</a>"#;
        assert!(html_to_md(html).contains("[text](url)"));

        let html = r#"<img src="img.png" alt="alt text">"#;
        assert!(html_to_md(html).contains("![alt text](img.png)"));
    }

    #[test]
    fn test_code() {
        // Inline code
        let html = "<p>Use <code>code</code> here</p>";
        assert!(html_to_md(html).contains("`code`"));

        // Code block
        let html = "<pre><code>block</code></pre>";
        assert!(html_to_md(html).contains("```\nblock\n```"));
    }

    #[test]
    fn test_lists() {
        // Unordered
        let html = "<ul><li>A</li><li>B</li></ul>";
        let md = html_to_md(html);
        assert!(md.contains("- A"));
        assert!(md.contains("- B"));

        // Ordered
        let html = "<ol><li>A</li><li>B</li></ol>";
        let md = html_to_md(html);
        assert!(md.contains("1. A"));
        assert!(md.contains("2. B"));
    }

    #[test]
    fn test_blockquote() {
        let html = "<blockquote><p>Quote</p></blockquote>";
        assert!(html_to_md(html).contains("> Quote"));
    }

    #[test]
    fn test_formatting() {
        assert!(html_to_md("<strong>bold</strong>").contains("**bold**"));
        assert!(html_to_md("<em>italic</em>").contains("*italic*"));
        assert!(html_to_md("<del>strike</del>").contains("~~strike~~"));
    }

    #[test]
    fn test_entities() {
        let html = "<p>&lt;tag&gt;</p>";
        let md = html_to_md(html);
        // The < and > are decoded then escaped for markdown
        assert!(md.contains("<") || md.contains("\\<"));
    }

    #[test]
    fn test_table() {
        let html = r#"
            <table>
                <thead><tr><th>A</th><th>B</th></tr></thead>
                <tbody><tr><td>1</td><td>2</td></tr></tbody>
            </table>
        "#;
        let md = html_to_md(html);
        assert!(md.contains("|"));
        assert!(md.contains("A"));
        assert!(md.contains("1"));
    }

    #[test]
    fn test_malformed_html() {
        // Missing closing tag
        let html = "<p>Text";
        let md = html_to_md(html);
        assert!(md.contains("Text"));

        // Missing opening tag
        let html = "Text</p>";
        let md = html_to_md(html);
        assert!(md.contains("Text"));
    }

    #[test]
    fn test_nested_structures() {
        // List inside blockquote
        let html = "<blockquote><ul><li>Item</li></ul></blockquote>";
        let md = html_to_md(html);
        assert!(md.contains(">"));
        assert!(md.contains("Item"));

        // Nested lists
        let html = "<ul><li>A<ul><li>B</li></ul></li></ul>";
        let md = html_to_md(html);
        assert!(md.contains("- A"));
        assert!(md.contains("- B"));
    }
}
