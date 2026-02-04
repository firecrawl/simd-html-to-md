//! Benchmarks for HTML to Markdown conversion.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use simd_html_to_md::{decode_entities, escape_markdown, html_to_md, Alignment, TableFormatter};

fn bench_simple_paragraph(c: &mut Criterion) {
    let html = "<p>Hello world, this is a simple paragraph.</p>";

    c.bench_function("simple_paragraph", |b| {
        b.iter(|| html_to_md(black_box(html)))
    });
}

fn bench_complex_document(c: &mut Criterion) {
    let html = r#"
<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
    <h1>Main Title</h1>
    <p>This is the first paragraph with <strong>bold</strong> and <em>italic</em> text.</p>

    <h2>Section 1</h2>
    <p>Here's a <a href="https://example.com">link</a> and some more text.</p>

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

    c.bench_function("complex_document", |b| {
        b.iter(|| html_to_md(black_box(html)))
    });
}

fn bench_large_document(c: &mut Criterion) {
    let mut html = String::new();
    html.push_str("<div>");
    for i in 0..100 {
        html.push_str(&format!(
            "<p>Paragraph {} with <strong>bold</strong> and <em>italic</em> text and a <a href=\"url{}\">link</a>.</p>",
            i, i
        ));
    }
    html.push_str("</div>");

    let mut group = c.benchmark_group("large_document");
    group.throughput(Throughput::Bytes(html.len() as u64));

    group.bench_function("100_paragraphs", |b| {
        b.iter(|| html_to_md(black_box(&html)))
    });

    group.finish();
}

fn bench_nested_lists(c: &mut Criterion) {
    let html = r#"
<ul>
    <li>Level 1 - Item 1
        <ul>
            <li>Level 2 - Item 1</li>
            <li>Level 2 - Item 2
                <ul>
                    <li>Level 3 - Item 1</li>
                    <li>Level 3 - Item 2</li>
                </ul>
            </li>
            <li>Level 2 - Item 3</li>
        </ul>
    </li>
    <li>Level 1 - Item 2</li>
    <li>Level 1 - Item 3
        <ul>
            <li>Level 2 - Item 1</li>
        </ul>
    </li>
</ul>
"#;

    c.bench_function("nested_lists", |b| {
        b.iter(|| html_to_md(black_box(html)))
    });
}

fn bench_table(c: &mut Criterion) {
    let mut html = String::from("<table><thead><tr><th>Col1</th><th>Col2</th><th>Col3</th><th>Col4</th></tr></thead><tbody>");
    for i in 0..50 {
        html.push_str(&format!(
            "<tr><td>Row {} Col 1</td><td>Row {} Col 2</td><td>Row {} Col 3</td><td>Row {} Col 4</td></tr>",
            i, i, i, i
        ));
    }
    html.push_str("</tbody></table>");

    c.bench_function("table_50_rows", |b| {
        b.iter(|| html_to_md(black_box(&html)))
    });
}

fn bench_entities(c: &mut Criterion) {
    let html = "<p>&lt;div&gt; &amp; &quot;quotes&quot; &copy; &reg; &trade; &mdash; &ndash; &nbsp;</p>".repeat(100);

    c.bench_function("entity_heavy", |b| {
        b.iter(|| html_to_md(black_box(&html)))
    });
}

fn bench_code_blocks(c: &mut Criterion) {
    let mut html = String::new();
    for i in 0..20 {
        html.push_str(&format!(
            r#"<pre><code class="language-rust">fn function_{}() {{
    let x = {};
    let y = x * 2;
    println!("Result: {{}}", y);
}}</code></pre>"#,
            i, i
        ));
    }

    c.bench_function("code_blocks_20", |b| {
        b.iter(|| html_to_md(black_box(&html)))
    });
}

fn bench_escape_markdown(c: &mut Criterion) {
    let text = "This is **bold** & `code` with [links](url) and <tags>!"
        .repeat(200);

    c.bench_function("escape_markdown_special", |b| {
        b.iter(|| escape_markdown(black_box(&text)))
    });
}

fn bench_decode_entities_micro(c: &mut Criterion) {
    let text = "&lt;div&gt; &amp; &quot;quotes&quot; &copy; &reg; &trade; &mdash; &ndash; &nbsp;"
        .repeat(200);

    c.bench_function("decode_entities_micro", |b| {
        b.iter(|| decode_entities(black_box(&text)))
    });
}

fn bench_table_formatter(c: &mut Criterion) {
    let mut table = TableFormatter::new();
    table.set_headers(vec![
        "Col1".to_string(),
        "Col2".to_string(),
        "Col3".to_string(),
        "Col4".to_string(),
    ]);

    table.set_alignment(1, Alignment::Center);
    table.set_alignment(3, Alignment::Right);

    for i in 0..50 {
        table.add_row(vec![
            format!("Row {} Col 1", i),
            format!("Row {} Col 2", i),
            format!("Row {} Col 3", i),
            format!("Row {} Col 4", i),
        ]);
    }

    c.bench_function("table_formatting_50_rows", |b| {
        b.iter(|| black_box(table.format()))
    });
}

criterion_group!(
    benches,
    bench_simple_paragraph,
    bench_complex_document,
    bench_large_document,
    bench_nested_lists,
    bench_table,
    bench_entities,
    bench_code_blocks,
    bench_escape_markdown,
    bench_decode_entities_micro,
    bench_table_formatter,
);
criterion_main!(benches);
