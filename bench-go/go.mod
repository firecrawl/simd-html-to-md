module bench-go

go 1.20

require github.com/firecrawl/html-to-markdown v0.0.0

replace github.com/firecrawl/html-to-markdown => ../../html-to-markdown

require (
	github.com/PuerkitoBio/goquery v1.9.2 // indirect
	github.com/andybalholm/cascadia v1.3.2 // indirect
	golang.org/x/net v0.25.0 // indirect
	gopkg.in/yaml.v2 v2.4.0 // indirect
)
