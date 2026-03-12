#!/usr/bin/env python3
"""Parse Go and Rust benchmark output and print a comparison table."""

# Go results (best of 3 runs) - ns/op and MB/s
go = {
    # Perf (large files)
    "big.html (1.7 MB)":        (182_091_903,  9.44),
    "big_list.html (1.5 MB)":   (133_191_146, 11.57),
    "table.html (1.1 MB)":      (134_564_516,  8.45),
    # Real-world sites
    "blog.golang.org (16 KB)":  (987_307,     17.01),
    "golang.org (8 KB)":        (259_275,     32.59),
    # Snippets
    "github_about":             (74_646,      49.26),
    "turndown_demo":            (83_833,      11.00),
    "tweet":                    (37_751,      17.22),
    # CommonMark
    "blockquote":               (72_958,      11.75),
    "bold":                     (68_311,      10.01),
    "list":                     (333_757,     11.27),
    "pre_code":                 (522_063,     15.32),
    "link":                     (139_715,     17.70),
    # Plugins
    "plugin/table":             (220_221,     10.15),
    "plugin/strikethrough":     (28_638,       6.91),
    "plugin/checkbox":          (44_812,      13.03),
}

# Rust results (median) - ns and MB/s
rust = {
    "big.html (1.7 MB)":       (5_191_800,   315.79),
    "big_list.html (1.5 MB)":  (8_573_600,   171.41),
    "table.html (1.1 MB)":     (5_564_100,   194.88),
    "blog.golang.org (16 KB)": (33_579,      476.88),
    "golang.org (8 KB)":       (9_055,       890.06),
    "github_about":            (3_314,       1055.0),
    "turndown_demo":           (2_870,       306.39),
    "tweet":                   (1_439,       430.93),
    "blockquote":              (1_707,       478.89),
    "bold":                    (1_945,       335.33),
    "list":                    (14_225,      252.15),
    "pre_code":                (16_214,      470.47),
    "link":                    (5_845,       403.51),
    "plugin/table":            (7_658,       278.34),
    "plugin/strikethrough":    (640,         294.90),
    "plugin/checkbox":         (2_457,       226.64),
}

print(f"{'Benchmark':<30} {'Go (ns)':<14} {'Rust (ns)':<14} {'Speedup':<10} {'Go MB/s':<10} {'Rust MB/s':<12}")
print("-" * 90)

for name in go:
    go_ns, go_mbs = go[name]
    rust_ns, rust_mbs = rust[name]
    speedup = go_ns / rust_ns
    print(f"{name:<30} {go_ns:>12,}  {rust_ns:>12,}  {speedup:>7.1f}x  {go_mbs:>8.1f}  {rust_mbs:>10.1f}")

# Summary
go_total = sum(v[0] for v in go.values())
rust_total = sum(v[0] for v in rust.values())
print("-" * 90)
print(f"{'TOTAL':<30} {go_total:>12,}  {rust_total:>12,}  {go_total/rust_total:>7.1f}x")
