//! CLI tool to clean up OCR markdown output.
//!
//! Strips image tags, converts leftover HTML to markdown,
//! and normalizes formatting.
//!
//! Usage:
//!   clean-ocr <input.md>
//!   cat input.md | clean-ocr

use std::io::{self, Read};

fn main() {
    let input = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("Error reading {}: {}", path, e);
            std::process::exit(1);
        }),
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
                eprintln!("Error reading stdin: {}", e);
                std::process::exit(1);
            });
            buf
        }
    };

    let output = clean_ocr_markdown(&input);
    print!("{}", output);
}

fn clean_ocr_markdown(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_html_block = false;
    let mut html_block = String::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Skip image-only lines: ![...](...)
        if is_image_line(trimmed) {
            continue;
        }

        // Strip inline images from lines that have other content
        let line = strip_inline_images(line);
        let trimmed = line.trim();

        // Skip empty lines that would result from image removal
        // (but preserve intentional blank lines in context)

        // Detect HTML block starts
        if !in_html_block && looks_like_html_block(trimmed) {
            in_html_block = true;
            html_block.clear();
            html_block.push_str(&line);
            html_block.push('\n');

            // Self-contained single-line HTML
            if is_self_contained_html(trimmed) {
                let converted = simd_html_to_md::html_to_md(&html_block);
                let converted = converted.trim();
                if !converted.is_empty() {
                    output.push_str(converted);
                    output.push('\n');
                }
                in_html_block = false;
                html_block.clear();
            }
            continue;
        }

        if in_html_block {
            html_block.push_str(&line);
            html_block.push('\n');

            // Check if HTML block ends (blank line or closing tag)
            if trimmed.is_empty() || looks_like_html_block_end(trimmed) {
                let converted = simd_html_to_md::html_to_md(&html_block);
                let converted = converted.trim();
                if !converted.is_empty() {
                    output.push_str(converted);
                    output.push('\n');
                }
                if trimmed.is_empty() {
                    output.push('\n');
                }
                in_html_block = false;
                html_block.clear();
            }
            continue;
        }

        // Pass through markdown lines, but convert any inline HTML fragments
        let cleaned = clean_inline_html(&line);
        output.push_str(&cleaned);
        output.push('\n');
    }

    // Flush any remaining HTML block
    if in_html_block && !html_block.is_empty() {
        let converted = simd_html_to_md::html_to_md(&html_block);
        let converted = converted.trim();
        if !converted.is_empty() {
            output.push_str(converted);
            output.push('\n');
        }
    }

    // Clean up excessive blank lines (3+ → 2)
    collapse_blank_lines(&output)
}

/// Check if a line is solely an image tag.
fn is_image_line(line: &str) -> bool {
    let s = line.trim();
    if !s.starts_with("![") {
        return false;
    }
    // ![alt](url) possibly followed by whitespace
    if let Some(paren_start) = s.find("](")
        && let Some(paren_end) = s[paren_start + 2..].find(')')
    {
        let after = &s[paren_start + 2 + paren_end + 1..].trim();
        return after.is_empty();
    }
    false
}

/// Strip ![alt](url) from a line that has other content too.
fn strip_inline_images(line: &str) -> String {
    if !line.contains("![") {
        return line.to_string();
    }

    let mut result = String::with_capacity(line.len());
    let mut i = 0;
    let bytes = line.as_bytes();

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'!' && bytes[i + 1] == b'[' {
            // Try to match ![...](...)
            if let Some(end) = find_image_end(line, i) {
                i = end;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

/// Find the end of an image tag starting at `start`.
fn find_image_end(s: &str, start: usize) -> Option<usize> {
    let rest = &s[start..];
    if !rest.starts_with("![") {
        return None;
    }
    let bracket_end = rest.find("](")?;
    let paren_end = rest[bracket_end + 2..].find(')')?;
    Some(start + bracket_end + 2 + paren_end + 1)
}

/// Check if a trimmed line looks like the start of an HTML block.
fn looks_like_html_block(line: &str) -> bool {
    line.starts_with('<')
        && !line.starts_with("<!--")
        && !line.starts_with("<br")
        && !line.starts_with("<hr")
}

/// Check if a single-line HTML block is self-contained.
fn is_self_contained_html(line: &str) -> bool {
    if !line.starts_with('<') {
        return false;
    }
    // Count opening and closing tags roughly
    let opens = line.matches('<').count() - line.matches("</").count() - line.matches("/>").count();
    let closes = line.matches("</").count() + line.matches("/>").count();
    closes >= opens
}

/// Check if line looks like end of an HTML block.
fn looks_like_html_block_end(line: &str) -> bool {
    line.starts_with("</") || line.ends_with("/>") || line.ends_with(">")
}

/// Convert inline HTML tags in an otherwise-markdown line.
fn clean_inline_html(line: &str) -> String {
    // If line has HTML tags mixed with markdown, convert the HTML parts
    if !line.contains('<') || line.trim().starts_with('|') {
        // No HTML or table row — pass through
        return line.to_string();
    }

    // If the line is mostly HTML (starts with <), convert the whole thing
    let trimmed = line.trim();
    if trimmed.starts_with('<') {
        return simd_html_to_md::html_to_md(trimmed)
            .trim_end_matches('\n')
            .to_string();
    }

    // Otherwise strip simple HTML tags like <br>, <br/>, <p>, etc.
    let mut result = String::with_capacity(line.len());
    let mut i = 0;
    let bytes = line.as_bytes();

    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(end) = find_tag_end(line, i)
        {
            let tag = &line[i..end];
            // Replace <br> / <br/> with newline, skip other simple tags
            if tag.starts_with("<br") {
                result.push('\n');
            } else if !tag.starts_with("</") && !tag.contains(' ') {
                // Simple opening tag like <p>, <div> — skip
            } else if tag.starts_with("</") {
                // Closing tag — skip
            }
            i = end;
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

fn find_tag_end(s: &str, start: usize) -> Option<usize> {
    let rest = &s[start..];
    rest.find('>').map(|pos| start + pos + 1)
}

/// Collapse 3+ consecutive blank lines into 2.
fn collapse_blank_lines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut blank_count = 0;

    for line in s.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line);
            result.push('\n');
        }
    }

    // Trim trailing whitespace
    while result.ends_with('\n') && result.len() > 1 && result[..result.len() - 1].ends_with('\n') {
        result.pop();
    }
    if !result.ends_with('\n') {
        result.push('\n');
    }

    result
}
