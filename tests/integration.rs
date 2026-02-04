//! Integration tests for HTML to Markdown conversion.

use simd_html_to_md::{html_to_md, html_to_md_with_options, Options};

#[test]
fn test_full_document() {
    let html = r#"
<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
    <h1>Main Title</h1>
    <p>This is the first paragraph with <strong>bold</strong> and <em>italic</em> text.</p>

    <h2>Section 1</h2>
    <p>Here's a <a href="https://example.com">link</a> and an image:</p>
    <img src="photo.jpg" alt="A photo">

    <h2>Section 2: Lists</h2>
    <ul>
        <li>First item</li>
        <li>Second item with <code>code</code></li>
        <li>Third item</li>
    </ul>

    <ol>
        <li>Numbered one</li>
        <li>Numbered two</li>
    </ol>

    <h2>Section 3: Code</h2>
    <pre><code class="language-rust">fn main() {
    println!("Hello, world!");
}</code></pre>

    <h2>Section 4: Quote</h2>
    <blockquote>
        <p>This is a famous quote.</p>
    </blockquote>

    <hr>

    <p>The end.</p>
</body>
</html>
"#;

    let md = html_to_md(html);

    // Check all major elements are converted
    assert!(md.contains("# Main Title"), "Missing h1");
    assert!(md.contains("**bold**"), "Missing bold");
    assert!(md.contains("*italic*"), "Missing italic");
    assert!(md.contains("[link](https://example.com)"), "Missing link");
    assert!(md.contains("![A photo](photo.jpg)"), "Missing image");
    assert!(md.contains("## Section 1"), "Missing h2");
    assert!(md.contains("- First item"), "Missing unordered list item");
    assert!(md.contains("`code`"), "Missing inline code");
    assert!(md.contains("1. Numbered one"), "Missing ordered list item");
    assert!(md.contains("```rust"), "Missing code block with language");
    assert!(md.contains("> This is a famous quote"), "Missing blockquote");
    assert!(md.contains("---"), "Missing horizontal rule");
}

#[test]
fn test_nested_lists() {
    let html = r#"
<ul>
    <li>Level 1 - Item 1
        <ul>
            <li>Level 2 - Item 1</li>
            <li>Level 2 - Item 2
                <ul>
                    <li>Level 3</li>
                </ul>
            </li>
        </ul>
    </li>
    <li>Level 1 - Item 2</li>
</ul>
"#;

    let md = html_to_md(html);

    assert!(md.contains("- Level 1 - Item 1"));
    assert!(md.contains("  - Level 2 - Item 1"));
    assert!(md.contains("  - Level 2 - Item 2"));
    assert!(md.contains("    - Level 3"));
    assert!(md.contains("- Level 1 - Item 2"));
}

#[test]
fn test_table() {
    let html = r#"
<table>
    <thead>
        <tr>
            <th>Name</th>
            <th>Age</th>
            <th>City</th>
        </tr>
    </thead>
    <tbody>
        <tr>
            <td>Alice</td>
            <td>30</td>
            <td>New York</td>
        </tr>
        <tr>
            <td>Bob</td>
            <td>25</td>
            <td>San Francisco</td>
        </tr>
    </tbody>
</table>
"#;

    let md = html_to_md(html);

    assert!(md.contains("| Name"));
    assert!(md.contains("| Age"));
    assert!(md.contains("| City"));
    assert!(md.contains("| Alice"));
    assert!(md.contains("| 30"));
    assert!(md.contains("| Bob"));
    assert!(md.contains("| San Francisco"));
    // Check for separator row
    assert!(md.contains("|---") || md.contains("| :--"));
}

#[test]
fn test_complex_inline_formatting() {
    let html = r#"<p>This is <strong>bold with <em>nested italic</em> inside</strong> text.</p>"#;
    let md = html_to_md(html);

    // The nested formatting should produce something reasonable
    assert!(md.contains("bold"));
    assert!(md.contains("nested italic"));
}

#[test]
fn test_link_with_title() {
    let html = r#"<a href="https://example.com" title="Example Site">Click here</a>"#;
    let md = html_to_md(html);

    assert!(md.contains("[Click here](https://example.com"));
    assert!(md.contains("\"Example Site\""));
}

#[test]
fn test_image_with_title() {
    let html = r#"<img src="photo.jpg" alt="Photo" title="My Photo">"#;
    let md = html_to_md(html);

    assert!(md.contains("![Photo](photo.jpg"));
    assert!(md.contains("\"My Photo\""));
}

#[test]
fn test_code_block_with_backticks() {
    let html = r#"<pre><code>let x = `template literal`;</code></pre>"#;
    let md = html_to_md(html);

    // Should handle backticks in code
    assert!(md.contains("```") || md.contains("~~~"));
    assert!(md.contains("`template literal`"));
}

#[test]
fn test_entity_decoding() {
    let html = r#"<p>Less than: &lt; Greater than: &gt; Ampersand: &amp;</p>"#;
    let md = html_to_md(html);

    // Entities should be decoded
    assert!(md.contains("<") || md.contains("\\<"));
    assert!(md.contains(">") || md.contains("\\>"));
    assert!(md.contains("&") || md.contains("\\&"));
}

#[test]
fn test_numeric_entities() {
    let html = r#"<p>&#60; &#62; &#38;</p>"#;
    let md = html_to_md(html);

    // Numeric entities should be decoded
    assert!(md.contains("<") || md.contains("\\<"));
}

#[test]
fn test_hex_entities() {
    let html = r#"<p>&#x3C; &#x3E; &#x26;</p>"#;
    let md = html_to_md(html);

    // Hex entities should be decoded
    assert!(md.contains("<") || md.contains("\\<"));
}

#[test]
fn test_strikethrough() {
    let html = r#"<p>This is <del>deleted</del> text.</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("~~deleted~~"));

    let html = r#"<p>This is <s>strikethrough</s> text.</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("~~strikethrough~~"));
}

#[test]
fn test_br_tag() {
    let html = r#"<p>Line one<br>Line two</p>"#;
    let md = html_to_md(html);

    // Should have proper line break
    assert!(md.contains("Line one"));
    assert!(md.contains("Line two"));
    // Two spaces before newline for Markdown line break
    assert!(md.contains("  \n"));
}

#[test]
fn test_hr_tag() {
    let html = r#"<p>Before</p><hr><p>After</p>"#;
    let md = html_to_md(html);

    assert!(md.contains("Before"));
    assert!(md.contains("---"));
    assert!(md.contains("After"));
}

#[test]
fn test_nested_blockquotes() {
    let html = r#"
<blockquote>
    <p>First level</p>
    <blockquote>
        <p>Second level</p>
    </blockquote>
</blockquote>
"#;

    let md = html_to_md(html);

    assert!(md.contains("> First level") || md.contains(">First level"));
    // Nested blockquote should have double prefix
    assert!(md.contains("> > Second level") || md.contains(">>"));
}

#[test]
fn test_whitespace_normalization() {
    let html = r#"<p>   Multiple    spaces    get    normalized   </p>"#;
    let md = html_to_md(html);

    // Should not have multiple consecutive spaces
    assert!(!md.contains("  ") || md.ends_with("  \n")); // Exception for br
    assert!(md.contains("Multiple spaces get normalized"));
}

#[test]
fn test_custom_options_bullet() {
    let options = Options {
        bullet_char: '*',
        ..Default::default()
    };

    let html = r#"<ul><li>Item 1</li><li>Item 2</li></ul>"#;
    let md = html_to_md_with_options(html, options);

    assert!(md.contains("* Item 1"));
    assert!(md.contains("* Item 2"));
}

#[test]
fn test_comments_ignored() {
    let html = r#"<p>Before</p><!-- This is a comment --><p>After</p>"#;
    let md = html_to_md(html);

    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    assert!(!md.contains("This is a comment"));
}

#[test]
fn test_doctype_ignored() {
    let html = r#"<!DOCTYPE html><p>Content</p>"#;
    let md = html_to_md(html);

    assert!(md.contains("Content"));
    assert!(!md.contains("DOCTYPE"));
}

#[test]
fn test_ordered_list_start() {
    let html = r#"<ol start="5"><li>Item</li><li>Item</li></ol>"#;
    let md = html_to_md(html);

    assert!(md.contains("5. Item"));
    assert!(md.contains("6. Item"));
}

#[test]
fn test_void_elements() {
    // These should be treated as self-closing
    let html = r#"<p>Before<br>After</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));

    let html = r#"<hr>"#;
    let md = html_to_md(html);
    assert!(md.contains("---"));
}

#[test]
fn test_malformed_unclosed_tags() {
    let html = r#"<p>Unclosed paragraph
<div>Another unclosed"#;

    // Should not panic and should extract text
    let md = html_to_md(html);
    assert!(md.contains("Unclosed paragraph") || md.contains("Another unclosed"));
}

#[test]
fn test_empty_elements() {
    let html = r#"<p></p><div></div>"#;
    let md = html_to_md(html);

    // Empty elements should produce minimal/no output
    assert!(md.is_empty() || md.trim().is_empty());
}

#[test]
fn test_script_and_style_as_containers() {
    // Script and style content should ideally be ignored
    // For now, they're treated as unknown containers
    let html = r#"<p>Before</p><script>alert('hi')</script><p>After</p>"#;
    let md = html_to_md(html);

    assert!(md.contains("Before"));
    assert!(md.contains("After"));
}

#[test]
fn test_large_input() {
    // Test with a larger input to verify SIMD performance
    let mut html = String::new();
    html.push_str("<div>");
    for i in 0..100 {
        html.push_str(&format!("<p>Paragraph {} with <strong>bold</strong> and <em>italic</em> text.</p>", i));
    }
    html.push_str("</div>");

    let md = html_to_md(&html);

    assert!(md.contains("Paragraph 0"));
    assert!(md.contains("Paragraph 99"));
    assert!(md.contains("**bold**"));
    assert!(md.contains("*italic*"));
}
