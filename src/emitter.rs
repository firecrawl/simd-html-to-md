//! Markdown output emitter with proper formatting.

use crate::simd;
use std::borrow::Cow;

/// Escape special Markdown characters in text content.
///
/// This prevents text from being interpreted as Markdown formatting.
/// Note: Characters like `-`, `+`, `.` are only special at line start,
/// but we escape them everywhere for safety. Consider context-aware
/// escaping for better output.
pub fn escape_markdown(text: &str) -> String {
    escape_markdown_cow(text).into_owned()
}

pub(crate) fn escape_markdown_cow(text: &str) -> Cow<'_, str> {
    const ESCAPE_SIMD_THRESHOLD: usize = 64;

    // Characters that definitely need escaping in Markdown:
    // \ ` * _ { } [ ] ( ) # ! | & < >
    // Characters that only need escaping at line start: + - .
    // We'll only escape the always-special ones in inline text
    let bytes = text.as_bytes();

    // Quick check: scan for any special characters
    let special = b"\\`*_{}[]()#!|<>&";
    let first_special = if bytes.len() >= ESCAPE_SIMD_THRESHOLD {
        simd::find_any_index(bytes, special)
    } else {
        find_first_special_scalar(bytes)
    };

    let first_special = match first_special {
        Some(pos) => pos,
        None => return Cow::Borrowed(text),
    };

    let mut result = String::with_capacity(text.len() + text.len() / 4);
    let mut last = 0;
    let mut pos = first_special;

    loop {
        if pos > last {
            result.push_str(&text[last..pos]);
        }
        result.push('\\');
        result.push(bytes[pos] as char);

        last = pos + 1;
        if last >= bytes.len() {
            break;
        }

        let next_rel = if bytes.len() - last >= ESCAPE_SIMD_THRESHOLD {
            simd::find_any_index(&bytes[last..], special)
        } else {
            find_first_special_scalar(&bytes[last..])
        };

        match next_rel {
            Some(rel) => pos = last + rel,
            None => {
                result.push_str(&text[last..]);
                break;
            }
        }
    }

    Cow::Owned(result)
}

fn find_first_special_scalar(bytes: &[u8]) -> Option<usize> {
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(
            b,
            b'\\'
                | b'`'
                | b'*'
                | b'_'
                | b'{'
                | b'}'
                | b'['
                | b']'
                | b'('
                | b')'
                | b'#'
                | b'!'
                | b'|'
                | b'<'
                | b'>'
                | b'&'
        ) {
            return Some(i);
        }
    }
    None
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

    for &b in text.as_bytes() {
        if b == b'`' {
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
    let level = level.clamp(1, 6) as usize;
    let text = text.trim();
    let mut result = String::with_capacity(level + 1 + text.len());
    for _ in 0..level {
        result.push('#');
    }
    result.push(' ');
    result.push_str(text);
    result
}

/// Format a link.
pub fn format_link(text: &str, url: &str, title: Option<&str>) -> String {
    let title_len = title.map_or(0, |t| t.len() + 3);
    let mut result = String::with_capacity(text.len() + url.len() + title_len + 4);
    result.push('[');
    result.push_str(text);
    result.push_str("](");
    result.push_str(url);
    if let Some(t) = title {
        result.push(' ');
        result.push('"');
        result.push_str(t);
        result.push('"');
    }
    result.push(')');
    result
}

/// Format an image.
pub fn format_image(alt: &str, src: &str, title: Option<&str>) -> String {
    let title_len = title.map_or(0, |t| t.len() + 3);
    let mut result = String::with_capacity(alt.len() + src.len() + title_len + 5);
    result.push('!');
    result.push('[');
    result.push_str(alt);
    result.push_str("](");
    result.push_str(src);
    if let Some(t) = title {
        result.push(' ');
        result.push('"');
        result.push_str(t);
        result.push('"');
    }
    result.push(')');
    result
}

/// Format a code block with optional language.
pub fn format_code_block(code: &str, language: Option<&str>) -> String {
    let fence = determine_fence(code);
    let lang = language.unwrap_or("");
    let code = code.trim_end();
    let mut result = String::with_capacity(fence.len() * 2 + lang.len() + code.len() + 2);
    result.push_str(fence);
    result.push_str(lang);
    result.push('\n');
    result.push_str(code);
    result.push('\n');
    result.push_str(fence);
    result
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
    let text = text.trim();

    // Handle multiline items
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");

    let mut result = String::with_capacity(text.len() + indent + 2);
    for _ in 0..indent {
        result.push(' ');
    }
    result.push(bullet);
    result.push(' ');
    result.push_str(first);

    // Continuation lines need extra indent
    let continuation_indent = indent + 2;
    for line in lines {
        result.push('\n');
        if line.is_empty() {
            // Keep empty lines but don't add trailing spaces
        } else {
            for _ in 0..continuation_indent {
                result.push(' ');
            }
            result.push_str(line);
        }
    }

    result
}

/// Format an ordered list item.
pub fn format_ordered_item(text: &str, number: usize, indent: usize) -> String {
    let text = text.trim();
    let marker = number.to_string();
    let marker_len = marker.len() + 1;

    // Handle multiline items
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");

    let mut result = String::with_capacity(text.len() + indent + marker_len + 2);
    for _ in 0..indent {
        result.push(' ');
    }
    result.push_str(&marker);
    result.push('.');
    result.push(' ');
    result.push_str(first);

    // Continuation lines need to align with content after the marker
    let continuation_indent = indent + marker_len + 1;
    for line in lines {
        result.push('\n');
        if !line.is_empty() {
            for _ in 0..continuation_indent {
                result.push(' ');
            }
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
    let mut result = String::with_capacity(text.len() + 4);
    result.push_str("**");
    result.push_str(text);
    result.push_str("**");
    result
}

/// Format emphasis.
pub fn format_emphasis(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 2);
    result.push('*');
    result.push_str(text);
    result.push('*');
    result
}

/// Format strikethrough (GFM).
pub fn format_strikethrough(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 4);
    result.push_str("~~");
    result.push_str(text);
    result.push_str("~~");
    result
}

/// Format inline code.
pub fn format_code(text: &str) -> String {
    let (escaped, wrapper_count) = escape_code_span(text);
    let mut result = String::with_capacity(escaped.len() + wrapper_count * 2);
    for _ in 0..wrapper_count {
        result.push('`');
    }
    result.push_str(&escaped);
    for _ in 0..wrapper_count {
        result.push('`');
    }
    result
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

fn append_table_cell(result: &mut String, text: &str, width: usize) {
    result.push(' ');
    result.push_str(text);
    let pad = width.saturating_sub(text.len());
    for _ in 0..pad {
        result.push(' ');
    }
    result.push(' ');
    result.push('|');
}

fn append_table_separator_cell(result: &mut String, width: usize, alignment: Alignment) {
    result.push(' ');
    match alignment {
        Alignment::Left => {
            result.push(':');
            for _ in 1..width {
                result.push('-');
            }
        }
        Alignment::Center => {
            result.push(':');
            if width > 2 {
                for _ in 0..(width - 2) {
                    result.push('-');
                }
            }
            result.push(':');
        }
        Alignment::Right => {
            if width > 1 {
                for _ in 0..(width - 1) {
                    result.push('-');
                }
            }
            result.push(':');
        }
    }
    result.push(' ');
    result.push('|');
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

        let total_width: usize = widths.iter().sum();
        let cols = widths.len();
        let row_len = 1 + total_width + (3 * cols);
        let total_rows = 2 + self.rows.len();
        let mut result = String::with_capacity(total_rows * (row_len + 1));

        // Header row
        result.push('|');
        for (i, header) in self.headers.iter().enumerate() {
            let width = widths.get(i).copied().unwrap_or(3);
            append_table_cell(&mut result, header, width);
        }
        result.push('\n');

        // Separator row
        result.push('|');
        for (i, &alignment) in self.alignments.iter().enumerate() {
            let width = widths.get(i).copied().unwrap_or(3);
            append_table_separator_cell(&mut result, width, alignment);
        }
        result.push('\n');

        // Data rows
        for row in &self.rows {
            result.push('|');
            for (i, cell) in row.iter().enumerate() {
                let width = widths.get(i).copied().unwrap_or(3);
                append_table_cell(&mut result, cell, width);
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

fn is_ascii_whitespace_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'\x0B')
}

/// Normalize whitespace in text (collapse multiple spaces/newlines).
pub fn normalize_whitespace(text: &str) -> String {
    if text.is_ascii() {
        let mut result = String::with_capacity(text.len());
        let mut last_was_space = false;
        for &b in text.as_bytes() {
            if is_ascii_whitespace_byte(b) {
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            } else {
                result.push(b as char);
                last_was_space = false;
            }
        }
        return result;
    }

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
#[allow(dead_code)]
pub fn clean_text(text: &str) -> String {
    clean_text_cow(text).into_owned()
}

pub(crate) fn clean_text_cow(text: &str) -> Cow<'_, str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Cow::Borrowed("");
    }

    if is_normalized_whitespace(trimmed) {
        return Cow::Borrowed(trimmed);
    }

    Cow::Owned(clean_text_owned(trimmed))
}

fn clean_text_owned(text: &str) -> String {
    // Check if there are any line break markers
    if !text.contains("  \n") {
        return normalize_whitespace(text);
    }

    // Preserve `  \n` sequences
    if text.is_ascii() {
        let mut result = String::with_capacity(text.len());
        let mut space_count = 0;

        for &b in text.as_bytes() {
            if b == b' ' {
                space_count += 1;
            } else if b == b'\n' {
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
                if space_count > 0
                    && !result.is_empty()
                    && !result.ends_with(' ')
                    && !result.ends_with('\n')
                {
                    result.push(' ');
                }
                space_count = 0;
                result.push(b as char);
            }
        }

        return result;
    }

    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
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

fn is_normalized_whitespace(text: &str) -> bool {
    if !text.is_ascii() {
        return false;
    }

    let mut prev_space = false;
    for &b in text.as_bytes() {
        if b == b' ' {
            if prev_space {
                return false;
            }
            prev_space = true;
        } else if matches!(b, b'\t' | b'\n' | b'\r' | b'\x0C' | b'\x0B') {
            return false;
        } else {
            prev_space = false;
        }
    }

    true
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
