// generate_golden converts every testdata HTML file with the Go converter
// (same config Firecrawl uses) and writes the output next to the input.
package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	md "github.com/firecrawl/html-to-markdown"
	"github.com/firecrawl/html-to-markdown/plugin"
)

func main() {
	testdata := filepath.Join("..", "testdata")

	var files []string
	err := filepath.Walk(testdata, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}
		if strings.HasSuffix(path, ".html") {
			files = append(files, path)
		}
		return nil
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "walk: %v\n", err)
		os.Exit(1)
	}

	for _, f := range files {
		data, err := os.ReadFile(f)
		if err != nil {
			fmt.Fprintf(os.Stderr, "read %s: %v\n", f, err)
			continue
		}
		html := string(data)

		conv := md.NewConverter("", true, nil)
		conv.Use(plugin.GitHubFlavored())

		out, err := conv.ConvertString(html)
		if err != nil {
			fmt.Fprintf(os.Stderr, "convert %s: %v\n", f, err)
			continue
		}

		outPath := strings.TrimSuffix(f, filepath.Ext(f)) + ".go-output.md"
		if strings.HasSuffix(f, "input.html") {
			outPath = filepath.Join(filepath.Dir(f), "go-output.md")
		}
		if err := os.WriteFile(outPath, []byte(out), 0644); err != nil {
			fmt.Fprintf(os.Stderr, "write %s: %v\n", outPath, err)
		}
		fmt.Printf("OK  %s\n", outPath)
	}
}
