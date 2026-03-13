//! Integration tests for HTML to Markdown conversion.

use simd_html_to_md::{Options, html_to_md, html_to_md_with_options};

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
    assert!(md.contains("_italic_"), "Missing italic");
    assert!(md.contains("[link](https://example.com)"), "Missing link");
    assert!(md.contains("![A photo](photo.jpg)"), "Missing image");
    assert!(md.contains("## Section 1"), "Missing h2");
    assert!(md.contains("- First item"), "Missing unordered list item");
    assert!(md.contains("`code`"), "Missing inline code");
    assert!(md.contains("1. Numbered one"), "Missing ordered list item");
    assert!(md.contains("```rust"), "Missing code block with language");
    assert!(
        md.contains("> This is a famous quote"),
        "Missing blockquote"
    );
    assert!(md.contains("* * *"), "Missing horizontal rule");
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
    assert!(md.contains("| --- |"));
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
    // <br> produces paragraph break (double newline) matching Go behavior
    assert!(md.contains("\n\n"));
}

#[test]
fn test_hr_tag() {
    let html = r#"<p>Before</p><hr><p>After</p>"#;
    let md = html_to_md(html);

    assert!(md.contains("Before"));
    assert!(md.contains("* * *"));
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
    assert!(md.contains("* * *"));
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
        html.push_str(&format!(
            "<p>Paragraph {} with <strong>bold</strong> and <em>italic</em> text.</p>",
            i
        ));
    }
    html.push_str("</div>");

    let md = html_to_md(&html);

    assert!(md.contains("Paragraph 0"));
    assert!(md.contains("Paragraph 99"));
    assert!(md.contains("**bold**"));
    assert!(md.contains("_italic_"));
}

// ---- New feature tests ----

#[test]
fn test_script_stripping() {
    let html = r#"<p>Before</p><script>alert('xss')</script><p>After</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    assert!(!md.contains("alert"));
    assert!(!md.contains("xss"));
}

#[test]
fn test_script_with_angle_brackets() {
    // Script content with < > that would confuse a naive parser
    let html = r#"<p>Before</p><script>if (a < b && c > d) { alert('hi'); }</script><p>After</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    assert!(!md.contains("alert"));
}

#[test]
fn test_style_stripping() {
    let html = r#"<p>Before</p><style>body { color: red; }</style><p>After</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    assert!(!md.contains("color"));
    assert!(!md.contains("red"));
}

#[test]
fn test_noscript_stripping() {
    let html = r#"<p>Before</p><noscript>Enable JavaScript</noscript><p>After</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    assert!(!md.contains("Enable JavaScript"));
}

#[test]
fn test_script_case_insensitive() {
    let html = r#"<p>Before</p><SCRIPT>alert('xss')</SCRIPT><p>After</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    assert!(!md.contains("alert"));
}

#[test]
fn test_custom_skip_tags() {
    let options = Options {
        skip_tags: vec![
            "script".to_string(),
            "style".to_string(),
            "noscript".to_string(),
            "nav".to_string(),
        ],
        ..Default::default()
    };
    let html =
        r#"<p>Content</p><nav><a href="/">Home</a><a href="/about">About</a></nav><p>More</p>"#;
    let md = html_to_md_with_options(html, options);
    assert!(md.contains("Content"));
    assert!(md.contains("More"));
    assert!(!md.contains("Home"));
    assert!(!md.contains("About"));
}

#[test]
fn test_empty_skip_tags() {
    // With empty skip_tags, script content is still stripped at tokenizer level
    // (raw text element handling) but converter won't skip anything extra
    let options = Options {
        skip_tags: vec![],
        ..Default::default()
    };
    let html = r#"<p>Before</p><script>alert('hi')</script><p>After</p>"#;
    let md = html_to_md_with_options(html, options);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    // Script content still stripped by tokenizer raw text handling
    assert!(!md.contains("alert"));
}

#[test]
fn test_gutter_stripping() {
    let html = r#"
<pre><code class="language-js"><table>
<tr><td class="gutter"><pre>1
2
3</pre></td><td class="code"><pre>const x = 1;
let y = 2;
return x + y;</pre></td></tr>
</table></code></pre>
"#;
    let md = html_to_md(html);
    assert!(md.contains("const x = 1;"));
    assert!(!md.contains("gutter"));
    // Line numbers from gutter should not appear
}

#[test]
fn test_line_numbers_stripping() {
    let html = r#"
<pre><code class="language-python"><div class="line-numbers-wrapper">
<span class="line-number">1</span>
<span class="line-number">2</span>
</div>def hello():
    print("world")</code></pre>
"#;
    let md = html_to_md(html);
    assert!(md.contains("def hello():"));
    // Line number content should be stripped
    assert!(!md.contains("line-number"));
}

#[test]
fn test_div_wrapped_code_lines() {
    // Syntax highlighters often wrap lines in divs
    let html = r#"
<pre><code class="language-js"><div class="line">const x = 1;</div><div class="line">let y = 2;</div><div class="line">return x + y;</div></code></pre>
"#;
    let md = html_to_md(html);
    assert!(md.contains("```js"));
    assert!(md.contains("const x = 1;\n"));
    assert!(md.contains("let y = 2;\n"));
    assert!(md.contains("return x + y;"));
}

#[test]
fn test_token_line_code_blocks() {
    // Docusaurus/Prism style with token-line divs
    let html = r#"
<pre><code class="language-rust"><div class="token-line"><span class="token keyword">fn</span> main() {</div><div class="token-line">    println!("hello");</div><div class="token-line">}</div></code></pre>
"#;
    let md = html_to_md(html);
    assert!(md.contains("```rust"));
    assert!(md.contains("fn main() {\n"));
    assert!(md.contains("    println!(\"hello\");\n"));
}

#[test]
fn test_lang_prefix_on_code() {
    let html = r#"<pre><code class="lang-javascript">var x = 1;</code></pre>"#;
    let md = html_to_md(html);
    assert!(md.contains("```javascript"));
    assert!(md.contains("var x = 1;"));
}

#[test]
fn test_language_prefix_on_pre() {
    // Language class on <pre> instead of <code>
    let html = r#"<pre class="language-python"><code>def foo(): pass</code></pre>"#;
    let md = html_to_md(html);
    assert!(md.contains("```python"));
    assert!(md.contains("def foo(): pass"));
}

#[test]
fn test_lang_prefix_on_pre() {
    let html = r#"<pre class="lang-go"><code>func main() {}</code></pre>"#;
    let md = html_to_md(html);
    assert!(md.contains("```go"));
    assert!(md.contains("func main() {}"));
}

#[test]
fn test_code_language_priority() {
    // <code> class takes priority over <pre> class
    let html =
        r#"<pre class="language-text"><code class="language-rust">fn main() {}</code></pre>"#;
    let md = html_to_md(html);
    assert!(md.contains("```rust"));
}

#[test]
fn test_skip_to_content_link() {
    let html = r##"<a href="#main-content">Skip to Content</a><h1>Title</h1>"##;
    let md = html_to_md(html);
    assert!(!md.contains("Skip to Content"));
    assert!(md.contains("# Title"));
}

#[test]
fn test_skip_to_main_content_link() {
    let html = r##"<a href="#content">Skip to Main Content</a><p>Body</p>"##;
    let md = html_to_md(html);
    assert!(!md.contains("Skip to Main Content"));
    assert!(md.contains("Body"));
}

#[test]
fn test_skip_to_navigation_link() {
    let html = r##"<a href="#nav">Skip to Navigation</a><p>Content</p>"##;
    let md = html_to_md(html);
    assert!(!md.contains("Skip to Navigation"));
    assert!(md.contains("Content"));
}

#[test]
fn test_normal_anchor_link_preserved() {
    // Non-skip-to links with # should be preserved
    let html = r##"<a href="#section-2">Section 2</a>"##;
    let md = html_to_md(html);
    assert!(md.contains("[Section 2](#section-2)"));
}

#[test]
fn test_normal_external_link_preserved() {
    // External links should never be stripped
    let html = r#"<a href="https://example.com">Skip to Content</a>"#;
    let md = html_to_md(html);
    assert!(md.contains("[Skip to Content](https://example.com)"));
}

#[test]
fn test_multiple_scripts_stripped() {
    let html = r#"<p>A</p><script>one()</script><p>B</p><script>two()</script><p>C</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("A"));
    assert!(md.contains("B"));
    assert!(md.contains("C"));
    assert!(!md.contains("one"));
    assert!(!md.contains("two"));
}

#[test]
fn test_script_with_attributes() {
    let html = r#"<p>Before</p><script type="text/javascript" src="app.js">var x = 1;</script><p>After</p>"#;
    let md = html_to_md(html);
    assert!(md.contains("Before"));
    assert!(md.contains("After"));
    assert!(!md.contains("var x"));
}

#[test]
fn test_aria_hidden_stripped() {
    // aria-hidden="true" elements should be completely skipped (e.g., decorative shadow text)
    let html = r#"<h2><div aria-hidden="true">Shadow text</div>Real heading</h2>"#;
    let md = html_to_md(html);
    assert!(md.contains("Real heading"));
    assert!(!md.contains("Shadow text"));
}

#[test]
fn test_aria_hidden_nested() {
    let html = r#"<div><div aria-hidden="true"><span>Hidden</span><p>Also hidden</p></div><p>Visible</p></div>"#;
    let md = html_to_md(html);
    assert!(md.contains("Visible"));
    assert!(!md.contains("Hidden"));
    assert!(!md.contains("Also hidden"));
}

#[test]
fn test_aria_hidden_false_not_stripped() {
    let html = r#"<div aria-hidden="false"><p>Should appear</p></div>"#;
    let md = html_to_md(html);
    assert!(md.contains("Should appear"));
}

#[test]
fn test_url_entity_decoding() {
    // &amp; in href attributes should be decoded to &
    let html = r#"<a href="/page?a=1&amp;b=2">Link</a>"#;
    let md = html_to_md(html);
    assert!(md.contains("/page?a=1&b=2"));
    assert!(!md.contains("&amp;"));
}

#[test]
fn test_image_src_entity_decoding() {
    let html = r#"<img src="/img?w=100&amp;h=200" alt="Photo">"#;
    let md = html_to_md(html);
    assert!(md.contains("/img?w=100&h=200"));
    assert!(!md.contains("&amp;"));
}

#[test]
fn test_gt_in_tailwind_class_attribute() {
    // Tailwind CSS classes like [&>span]:px-6 contain > inside quoted attribute values.
    // The tokenizer must not treat them as tag close.
    let html = r#"<a href="https://example.com"><button class="[&amp;>span]:px-6 flex items-center button-primary [&amp;>*]:relative text-label-medium" type="button">Click me</button></a>"#;
    let md = html_to_md(html);
    assert!(md.contains("[Click me](https://example.com)"));
    // CSS class names should NOT appear in the output
    assert!(!md.contains("button-primary"));
    assert!(!md.contains("text-label-medium"));
    assert!(!md.contains("items-center"));
}

#[test]
fn test_newlines_escaped_in_link_text() {
    // <br> inside link text should produce escaped newlines, not bare newlines
    // which would break the markdown link syntax.
    let html = r#"<a href="https://example.com">Line one<br>Line two</a>"#;
    let md = html_to_md(html);
    assert!(md.contains(r"[Line one\"));
    assert!(md.contains("Line two](https://example.com)"));
    // Must NOT have a bare newline breaking the link
    assert!(!md.contains("[Line one\nLine two]"));
}

#[test]
fn test_newlines_escaped_in_link_with_block_elements() {
    // Block elements like <p> and <div> inside links should not break the link
    let html = r#"<a href="/url"><p>First paragraph</p><p>Second paragraph</p></a>"#;
    let md = html_to_md(html);
    // Link must stay intact — both paragraphs inside one [...](...) link
    assert!(md.contains("First paragraph"));
    assert!(md.contains("Second paragraph"));
    assert!(md.contains("](/url)"));
    // Must NOT have a bare newline breaking the link
    assert!(!md.contains("[First paragraph\nSecond"));
}

#[test]
fn test_gt_in_attribute_does_not_leak() {
    // Multiple buttons with > in class attributes inside links
    let html = r#"<a href="/a"><button class="[&amp;>*]:rel text-lg">A</button></a> <a href="/b"><button class="[&amp;>div]:flex">B</button></a>"#;
    let md = html_to_md(html);
    assert!(md.contains("[A](/a)"));
    assert!(md.contains("[B](/b)"));
    assert!(!md.contains(":rel"));
    assert!(!md.contains(":flex"));
}
