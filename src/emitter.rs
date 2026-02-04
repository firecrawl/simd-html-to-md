//! Markdown output emitter with proper formatting.

/// Escape special Markdown characters in text content.
///
/// This prevents text from being interpreted as Markdown formatting.
/// Note: Characters like `-`, `+`, `.` are only special at line start,
/// but we escape them everywhere for safety. Consider context-aware
/// escaping for better output.
pub fn escape_markdown(text: &str) -> String {
    // Characters that definitely need escaping in Markdown:
    // \ ` * _ { } [ ] ( ) # ! | & < >
    // Characters that only need escaping at line start: + - .
    // We'll only escape the always-special ones in inline text
    let bytes = text.as_bytes();

    // Quick check: scan for any special characters
    let special = b"\\`*_{}[]()#!|<>&";
    let mut needs_escape = false;
    for &b in bytes {
        if special.contains(&b) {
            needs_escape = true;
            break;
        }
    }

    if !needs_escape {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len() + text.len() / 4);

    for c in text.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#'
            | '!' | '|' | '<' | '>' | '&' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }

    result
}

/// Escape text for use inside a code span (backticks).
/// The only thing we need to handle is backticks themselves.
pub fn escape_code_span(text: &str) -> (String, usize) {
    // Count consecutive backticks to determine wrapping
    let max_consecutive = count_max_consecutive_backticks(text);

    // Use one more backtick than the maximum found
    let wrapper_count = max_consecutive + 1;

    // If the text starts or ends with a backtick, we need spaces
    let needs_space = text.starts_with('`') || text.ends_with('`') || text.starts_with(' ') && text.ends_with(' ');

    let result = if needs_space {
        format!(" {} ", text)
    } else {
        text.to_string()
    };

    (result, wrapper_count)
}

/// Count the maximum number of consecutive backticks in a string.
fn count_max_consecutive_backticks(text: &str) -> usize {
    let mut max = 0;
    let mut current = 0;

    for c in text.chars() {
        if c == '`' {
            current += 1;
            max = max.max(current);
        } else {
            current = 0;
        }
    }

    max
}

/// Format a heading with ATX style.
pub fn format_heading(level: u8, text: &str) -> String {
    let level = level.clamp(1, 6);
    let hashes = "#".repeat(level as usize);
    format!("{} {}", hashes, text.trim())
}

/// Format a link.
pub fn format_link(text: &str, url: &str, title: Option<&str>) -> String {
    match title {
        Some(t) => format!("[{}]({} \"{}\")", text, url, t),
        None => format!("[{}]({})", text, url),
    }
}

/// Format an image.
pub fn format_image(alt: &str, src: &str, title: Option<&str>) -> String {
    match title {
        Some(t) => format!("![{}]({} \"{}\")", alt, src, t),
        None => format!("![{}]({})", alt, src),
    }
}

/// Format a code block with optional language.
pub fn format_code_block(code: &str, language: Option<&str>) -> String {
    let fence = determine_fence(code);
    let lang = language.unwrap_or("");

    format!("{}{}\n{}\n{}", fence, lang, code.trim_end(), fence)
}

/// Determine the fence characters to use for a code block.
/// Uses ``` unless the code contains ```, then uses ~~~.
fn determine_fence(code: &str) -> &'static str {
    if code.contains("```") {
        "~~~"
    } else {
        "```"
    }
}

/// Format a blockquote by prefixing each line with `> `.
#[allow(dead_code)]
pub fn format_blockquote(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {}", line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format an unordered list item.
pub fn format_unordered_item(text: &str, indent: usize, bullet: char) -> String {
    let prefix = " ".repeat(indent);
    let text = text.trim();

    // Handle multiline items
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let mut result = format!("{}{} {}", prefix, bullet, first);

    // Continuation lines need extra indent
    let continuation_prefix = " ".repeat(indent + 2);
    for line in lines {
        result.push('\n');
        if line.is_empty() {
            // Keep empty lines but don't add trailing spaces
        } else {
            result.push_str(&continuation_prefix);
            result.push_str(line);
        }
    }

    result
}

/// Format an ordered list item.
pub fn format_ordered_item(text: &str, number: usize, indent: usize) -> String {
    let prefix = " ".repeat(indent);
    let text = text.trim();
    let marker = format!("{}.", number);
    let marker_len = marker.len();

    // Handle multiline items
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let mut result = format!("{}{} {}", prefix, marker, first);

    // Continuation lines need to align with content after the marker
    let continuation_prefix = " ".repeat(indent + marker_len + 1);
    for line in lines {
        result.push('\n');
        if !line.is_empty() {
            result.push_str(&continuation_prefix);
            result.push_str(line);
        }
    }

    result
}

/// Format a horizontal rule.
pub fn format_hr() -> &'static str {
    "---"
}

/// Format a line break.
pub fn format_br() -> &'static str {
    "  \n"
}

/// Format strong emphasis.
pub fn format_strong(text: &str) -> String {
    format!("**{}**", text)
}

/// Format emphasis.
pub fn format_emphasis(text: &str) -> String {
    format!("*{}*", text)
}

/// Format strikethrough (GFM).
pub fn format_strikethrough(text: &str) -> String {
    format!("~~{}~~", text)
}

/// Format inline code.
pub fn format_code(text: &str) -> String {
    let (escaped, wrapper_count) = escape_code_span(text);
    let backticks = "`".repeat(wrapper_count);
    format!("{}{}{}", backticks, escaped, backticks)
}

/// A table formatter for GFM tables.
#[derive(Debug)]
pub struct TableFormatter {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    alignments: Vec<Alignment>,
}

/// Column alignment for tables.
#[derive(Debug, Clone, Copy, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

impl TableFormatter {
    /// Create a new table formatter.
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            alignments: Vec::new(),
        }
    }

    /// Set the headers.
    pub fn set_headers(&mut self, headers: Vec<String>) {
        self.alignments = vec![Alignment::Left; headers.len()];
        self.headers = headers;
    }

    /// Add a row.
    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    /// Set alignment for a column.
    pub fn set_alignment(&mut self, col: usize, alignment: Alignment) {
        if col < self.alignments.len() {
            self.alignments[col] = alignment;
        }
    }

    /// Format the table.
    pub fn format(&self) -> String {
        if self.headers.is_empty() {
            return String::new();
        }

        // Calculate column widths
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.len().max(3)).collect();

        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        let mut result = String::new();

        // Header row
        result.push('|');
        for (i, header) in self.headers.iter().enumerate() {
            let width = widths.get(i).copied().unwrap_or(3);
            result.push_str(&format!(" {:width$} |", header, width = width));
        }
        result.push('\n');

        // Separator row
        result.push('|');
        for (i, &alignment) in self.alignments.iter().enumerate() {
            let width = widths.get(i).copied().unwrap_or(3);
            let sep = match alignment {
                Alignment::Left => format!(":{}", "-".repeat(width - 1)),
                Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
                Alignment::Right => format!("{}:", "-".repeat(width - 1)),
            };
            result.push_str(&format!(" {} |", sep));
        }
        result.push('\n');

        // Data rows
        for row in &self.rows {
            result.push('|');
            for (i, cell) in row.iter().enumerate() {
                let width = widths.get(i).copied().unwrap_or(3);
                result.push_str(&format!(" {:width$} |", cell, width = width));
            }
            result.push('\n');
        }

        // Remove trailing newline
        result.pop();

        result
    }
}

impl Default for TableFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize whitespace in text (collapse multiple spaces/newlines).
pub fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_was_space = false;

    for c in text.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }

    result
}

/// Trim leading/trailing whitespace and collapse internal whitespace.
/// Preserves Markdown line breaks (`  \n`).
pub fn clean_text(text: &str) -> String {
    let text = text.trim();

    // Check if there are any line break markers
    if !text.contains("  \n") {
        return normalize_whitespace(text);
    }

    // Preserve `  \n` sequences
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut space_count = 0;

    while let Some(c) = chars.next() {
        if c == ' ' {
            space_count += 1;
        } else if c == '\n' {
            if space_count >= 2 {
                // This is a Markdown line break
                result.push_str("  \n");
            } else if space_count > 0 || !result.is_empty() {
                // Regular whitespace
                if !result.ends_with(' ') && !result.ends_with('\n') {
                    result.push(' ');
                }
            }
            space_count = 0;
        } else {
            if space_count > 0 && !result.is_empty() && !result.ends_with(' ') && !result.ends_with('\n') {
                result.push(' ');
            }
            space_count = 0;
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_markdown() {
        assert_eq!(escape_markdown("hello world"), "hello world");
        assert_eq!(escape_markdown("*bold*"), "\\*bold\\*");
        assert_eq!(escape_markdown("[link](url)"), "\\[link\\]\\(url\\)");
        assert_eq!(escape_markdown("# heading"), "\\# heading");
    }

    #[test]
    fn test_format_heading() {
        assert_eq!(format_heading(1, "Title"), "# Title");
        assert_eq!(format_heading(2, "Subtitle"), "## Subtitle");
        assert_eq!(format_heading(6, "H6"), "###### H6");
        assert_eq!(format_heading(7, "Clamped"), "###### Clamped"); // Clamped to 6
    }

    #[test]
    fn test_format_link() {
        assert_eq!(format_link("text", "url", None), "[text](url)");
        assert_eq!(
            format_link("text", "url", Some("title")),
            "[text](url \"title\")"
        );
    }

    #[test]
    fn test_format_image() {
        assert_eq!(format_image("alt", "src", None), "![alt](src)");
        assert_eq!(
            format_image("alt", "src", Some("title")),
            "![alt](src \"title\")"
        );
    }

    #[test]
    fn test_format_code_block() {
        assert_eq!(format_code_block("code", None), "```\ncode\n```");
        assert_eq!(
            format_code_block("code", Some("rust")),
            "```rust\ncode\n```"
        );
    }

    #[test]
    fn test_format_code_block_with_backticks() {
        let code = "let x = `backticks`";
        assert_eq!(
            format_code_block(code, None),
            "```\nlet x = `backticks`\n```"
        );

        let code_with_fence = "```\ncode\n```";
        assert_eq!(
            format_code_block(code_with_fence, None),
            "~~~\n```\ncode\n```\n~~~"
        );
    }

    #[test]
    fn test_format_blockquote() {
        assert_eq!(format_blockquote("hello"), "> hello");
        assert_eq!(format_blockquote("line1\nline2"), "> line1\n> line2");
        assert_eq!(format_blockquote("line1\n\nline2"), "> line1\n>\n> line2");
    }

    #[test]
    fn test_format_unordered_item() {
        assert_eq!(format_unordered_item("item", 0, '-'), "- item");
        assert_eq!(format_unordered_item("item", 2, '-'), "  - item");
        assert_eq!(format_unordered_item("item", 0, '*'), "* item");
    }

    #[test]
    fn test_format_ordered_item() {
        assert_eq!(format_ordered_item("item", 1, 0), "1. item");
        assert_eq!(format_ordered_item("item", 10, 0), "10. item");
        assert_eq!(format_ordered_item("item", 1, 2), "  1. item");
    }

    #[test]
    fn test_format_code() {
        assert_eq!(format_code("code"), "`code`");
        assert_eq!(format_code("code`with`backticks"), "``code`with`backticks``");
        assert_eq!(format_code("`start"), "`` `start ``");
    }

    #[test]
    fn test_table_formatter() {
        let mut table = TableFormatter::new();
        table.set_headers(vec!["Name".to_string(), "Age".to_string()]);
        table.add_row(vec!["Alice".to_string(), "30".to_string()]);
        table.add_row(vec!["Bob".to_string(), "25".to_string()]);

        let expected = "| Name  | Age |\n| :---- | :-- |\n| Alice | 30  |\n| Bob   | 25  |";
        assert_eq!(table.format(), expected);
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("hello  world"), "hello world");
        assert_eq!(normalize_whitespace("hello\n\nworld"), "hello world");
        assert_eq!(normalize_whitespace("  hello  "), " hello ");
    }

    #[test]
    fn test_clean_text() {
        assert_eq!(clean_text("  hello  world  "), "hello world");
        assert_eq!(clean_text("\n\nhello\n\n"), "hello");
    }
}
