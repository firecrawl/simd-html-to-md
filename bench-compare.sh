#!/usr/bin/env bash
#
# Run Go and Rust benchmarks side-by-side and print a comparison table.
#
set -euo pipefail
cd "$(dirname "$0")"

SEP="=================================================================="

echo "$SEP"
echo "  Go html-to-markdown benchmarks (release / default optimizations)"
echo "$SEP"
echo ""
(cd bench-go && go test -bench=. -benchmem -count=3 -timeout=600s) 2>&1 | tee /tmp/go_bench.txt
echo ""

echo "$SEP"
echo "  Rust simd-html-to-md benchmarks (--release)"
echo "$SEP"
echo ""
cargo +nightly bench --bench realworld 2>&1 | tee /tmp/rust_bench.txt
echo ""

echo "$SEP"
echo "  Comparison Summary"
echo "$SEP"
echo ""
echo "Go results (best of 3):"
grep -E '^Benchmark' /tmp/go_bench.txt | sort | head -20
echo ""
echo "Rust results (see detailed output above for statistical analysis)"
echo ""
