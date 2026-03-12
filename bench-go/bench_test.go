package main

import (
	"os"
	"path/filepath"
	"testing"

	md "github.com/firecrawl/html-to-markdown"
	"github.com/firecrawl/html-to-markdown/plugin"
)

var testdataDir = filepath.Join("..", "testdata")

func loadFile(b *testing.B, name string) string {
	b.Helper()
	data, err := os.ReadFile(filepath.Join(testdataDir, name))
	if err != nil {
		b.Fatalf("read %s: %v", name, err)
	}
	return string(data)
}

func newConverter() *md.Converter {
	conv := md.NewConverter("", true, nil)
	conv.Use(plugin.GitHubFlavored())
	return conv
}

// ---------------------------------------------------------------------------
// Large performance files
// ---------------------------------------------------------------------------

func BenchmarkPerf_big_html(b *testing.B) {
	html := loadFile(b, "Perf/big.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkPerf_big_list(b *testing.B) {
	html := loadFile(b, "Perf/big_list.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkPerf_table(b *testing.B) {
	html := loadFile(b, "Perf/table.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

// ---------------------------------------------------------------------------
// Real-world sites
// ---------------------------------------------------------------------------

func BenchmarkRealworld_blog_golang_org(b *testing.B) {
	html := loadFile(b, "TestRealWorld/blog.golang.org/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkRealworld_golang_org(b *testing.B) {
	html := loadFile(b, "TestRealWorld/golang.org/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

// ---------------------------------------------------------------------------
// Real-world snippets
// ---------------------------------------------------------------------------

func BenchmarkSnippet_github_about(b *testing.B) {
	html := loadFile(b, "TestRealWorld/snippets/github_about/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkSnippet_turndown_demo(b *testing.B) {
	html := loadFile(b, "TestRealWorld/snippets/turndown_demo/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkSnippet_tweet(b *testing.B) {
	html := loadFile(b, "TestRealWorld/snippets/tweet/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

// ---------------------------------------------------------------------------
// CommonMark
// ---------------------------------------------------------------------------

func BenchmarkCommonmark_blockquote(b *testing.B) {
	html := loadFile(b, "TestCommonmark/blockquote/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkCommonmark_bold(b *testing.B) {
	html := loadFile(b, "TestCommonmark/bold/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkCommonmark_list(b *testing.B) {
	html := loadFile(b, "TestCommonmark/list/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkCommonmark_pre_code(b *testing.B) {
	html := loadFile(b, "TestCommonmark/pre_code/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkCommonmark_link(b *testing.B) {
	html := loadFile(b, "TestCommonmark/link/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

// ---------------------------------------------------------------------------
// Plugin test inputs
// ---------------------------------------------------------------------------

func BenchmarkPlugin_table(b *testing.B) {
	html := loadFile(b, "TestPlugins/table/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkPlugin_strikethrough(b *testing.B) {
	html := loadFile(b, "TestPlugins/strikethrough/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}

func BenchmarkPlugin_checkbox(b *testing.B) {
	html := loadFile(b, "TestPlugins/checkbox/input.html")
	b.SetBytes(int64(len(html)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		conv := newConverter()
		out, err := conv.ConvertString(html)
		if err != nil {
			b.Fatal(err)
		}
		_ = out
	}
}
