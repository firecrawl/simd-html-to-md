# simd-html-to-md

SIMD-accelerated HTML to Markdown converter in Rust. Single-pass streaming tokenizer with no DOM allocation — designed for high-throughput server-side conversion.

## Usage

```rust
use simd_html_to_md::html_to_md;

let html = "<h1>Hello</h1><p>This is <strong>bold</strong> text.</p>";
let md = html_to_md(html);
// # Hello
//
// This is **bold** text.
```

### Options

```rust
use simd_html_to_md::{html_to_md_with_options, Options};

let options = Options {
    bullet_char: '*',
    skip_tags: vec!["nav".into(), "footer".into()],
    ..Default::default()
};

let md = html_to_md_with_options(html, options);
```

## Features

- SIMD-accelerated byte scanning (`std::simd` portable SIMD)
- Single-pass streaming — no DOM tree, no per-node allocations
- GFM tables with alignment
- Fenced code blocks with language detection (`language-*`, `lang-*`)
- Syntax highlighter support (gutter/line-number stripping)
- HTML entity decoding (named, decimal, hex)
- Configurable tag skipping (script, style, noscript by default)
- Skip-to-content link removal

## Requirements

Requires Rust nightly (`#![feature(portable_simd)]`).

## Benchmarks

```
blog.golang.org (8KB)    38 µs
large table (1.1MB)       7 ms
stress test (1.7MB)     4.5 ms
```

```sh
cargo +nightly bench
```

## Tests

```sh
cargo +nightly test
```
