//! Real-world benchmarks using testdata from the Go html-to-markdown project.
//!
//! These benchmarks exercise the converter on the same HTML files the Go
//! library uses, so timings are directly comparable.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use simd_html_to_md::html_to_md;
use std::fs;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

// ---------------------------------------------------------------------------
// Large performance files (same files the Go benchmarks use)
// ---------------------------------------------------------------------------

fn bench_perf_big_html(c: &mut Criterion) {
    let html = load("testdata/Perf/big.html");
    let mut g = c.benchmark_group("perf");
    g.throughput(Throughput::Bytes(html.len() as u64));
    g.bench_function(BenchmarkId::new("big.html", html.len()), |b| {
        b.iter(|| html_to_md(black_box(&html)))
    });
    g.finish();
}

fn bench_perf_big_list(c: &mut Criterion) {
    let html = load("testdata/Perf/big_list.html");
    let mut g = c.benchmark_group("perf");
    g.throughput(Throughput::Bytes(html.len() as u64));
    g.bench_function(BenchmarkId::new("big_list.html", html.len()), |b| {
        b.iter(|| html_to_md(black_box(&html)))
    });
    g.finish();
}

fn bench_perf_table(c: &mut Criterion) {
    let html = load("testdata/Perf/table.html");
    let mut g = c.benchmark_group("perf");
    g.throughput(Throughput::Bytes(html.len() as u64));
    g.bench_function(BenchmarkId::new("table.html", html.len()), |b| {
        b.iter(|| html_to_md(black_box(&html)))
    });
    g.finish();
}

// ---------------------------------------------------------------------------
// Real-world website pages
// ---------------------------------------------------------------------------

fn bench_realworld_sites(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        (
            "blog.golang.org",
            "testdata/TestRealWorld/blog.golang.org/input.html",
        ),
        ("golang.org", "testdata/TestRealWorld/golang.org/input.html"),
    ];

    let mut g = c.benchmark_group("realworld_sites");
    for &(name, path) in cases {
        let html = load(path);
        g.throughput(Throughput::Bytes(html.len() as u64));
        g.bench_with_input(BenchmarkId::new(name, html.len()), &html, |b, html| {
            b.iter(|| html_to_md(black_box(html)))
        });
    }
    g.finish();
}

// ---------------------------------------------------------------------------
// Real-world snippets (small, but realistic patterns)
// ---------------------------------------------------------------------------

fn bench_realworld_snippets(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        (
            "github_about",
            "testdata/TestRealWorld/snippets/github_about/input.html",
        ),
        (
            "heading_in_link",
            "testdata/TestRealWorld/snippets/heading_in_link/input.html",
        ),
        (
            "nav_nested_list",
            "testdata/TestRealWorld/snippets/nav_nested_list/input.html",
        ),
        (
            "pre_code",
            "testdata/TestRealWorld/snippets/pre_code/input.html",
        ),
        (
            "price_em_in_a_p",
            "testdata/TestRealWorld/snippets/price_em_in_a_p/input.html",
        ),
        (
            "square_brackets",
            "testdata/TestRealWorld/snippets/square_brackets/input.html",
        ),
        (
            "text_with_whitespace",
            "testdata/TestRealWorld/snippets/text_with_whitespace/input.html",
        ),
        (
            "turndown_demo",
            "testdata/TestRealWorld/snippets/turndown_demo/input.html",
        ),
        ("tweet", "testdata/TestRealWorld/snippets/tweet/input.html"),
    ];

    let mut g = c.benchmark_group("realworld_snippets");
    for &(name, path) in cases {
        let html = load(path);
        g.throughput(Throughput::Bytes(html.len() as u64));
        g.bench_with_input(BenchmarkId::new(name, html.len()), &html, |b, html| {
            b.iter(|| html_to_md(black_box(html)))
        });
    }
    g.finish();
}

// ---------------------------------------------------------------------------
// CommonMark conformance inputs
// ---------------------------------------------------------------------------

fn bench_commonmark(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        (
            "blockquote",
            "testdata/TestCommonmark/blockquote/input.html",
        ),
        ("bold", "testdata/TestCommonmark/bold/input.html"),
        (
            "br_element",
            "testdata/TestCommonmark/br_element/input.html",
        ),
        ("heading", "testdata/TestCommonmark/heading/input.html"),
        ("hr", "testdata/TestCommonmark/hr/input.html"),
        ("image", "testdata/TestCommonmark/image/input.html"),
        ("italic", "testdata/TestCommonmark/italic/input.html"),
        ("link", "testdata/TestCommonmark/link/input.html"),
        ("list", "testdata/TestCommonmark/list/input.html"),
        (
            "list_nested",
            "testdata/TestCommonmark/list_nested/input.html",
        ),
        ("p_tag", "testdata/TestCommonmark/p_tag/input.html"),
        ("pre_code", "testdata/TestCommonmark/pre_code/input.html"),
    ];

    let mut g = c.benchmark_group("commonmark");
    for &(name, path) in cases {
        let html = load(path);
        g.throughput(Throughput::Bytes(html.len() as u64));
        g.bench_with_input(BenchmarkId::new(name, html.len()), &html, |b, html| {
            b.iter(|| html_to_md(black_box(html)))
        });
    }
    g.finish();
}

// ---------------------------------------------------------------------------
// Plugin test inputs (table, strikethrough, checkbox)
// ---------------------------------------------------------------------------

fn bench_plugins(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        ("table", "testdata/TestPlugins/table/input.html"),
        (
            "strikethrough",
            "testdata/TestPlugins/strikethrough/input.html",
        ),
        ("checkbox", "testdata/TestPlugins/checkbox/input.html"),
    ];

    let mut g = c.benchmark_group("plugins");
    for &(name, path) in cases {
        let html = load(path);
        g.throughput(Throughput::Bytes(html.len() as u64));
        g.bench_with_input(BenchmarkId::new(name, html.len()), &html, |b, html| {
            b.iter(|| html_to_md(black_box(html)))
        });
    }
    g.finish();
}

// ---------------------------------------------------------------------------
// Criterion setup
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_perf_big_html,
    bench_perf_big_list,
    bench_perf_table,
    bench_realworld_sites,
    bench_realworld_snippets,
    bench_commonmark,
    bench_plugins,
);
criterion_main!(benches);
