//! HTML tokenizer using SIMD-accelerated scanning.

use crate::simd;

/// An HTML token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token<'a> {
    /// Start tag like `<div class="foo">`
    StartTag(Tag<'a>),
    /// End tag like `</div>`
    EndTag(&'a str),
    /// Self-closing tag like `<br/>` or `<img src="x"/>`
    SelfClosingTag(Tag<'a>),
    /// Text content between tags
    Text(&'a str),
    /// HTML comment `<!-- comment -->`
    Comment(&'a str),
    /// DOCTYPE declaration `<!DOCTYPE html>`
    Doctype(&'a str),
}

/// Represents an HTML tag with its name and attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag<'a> {
    pub name: &'a str,
    pub attributes: Vec<Attribute<'a>>,
}

/// An HTML attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute<'a> {
    pub name: &'a str,
    pub value: Option<&'a str>,
}

const TAG_NAME_DELIMS: &[u8] = b" \t\n\r\x0C\x0B";
const ATTR_NAME_DELIMS: &[u8] = b" \t\n\r\x0C\x0B=/>"; // whitespace + '=' '/' '>'
const ATTR_VALUE_DELIMS: &[u8] = b" \t\n\r\x0C\x0B/>"; // whitespace + '/' '>'

impl<'a> Tag<'a> {
    /// Get the value of an attribute by name (case-insensitive).
    pub fn get_attr(&self, name: &str) -> Option<&'a str> {
        self.attributes
            .iter()
            .find(|attr| attr.name.eq_ignore_ascii_case(name))
            .and_then(|attr| attr.value)
    }
}

/// HTML tokenizer.
pub struct Tokenizer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    /// Pending end tag to emit (from raw text element handling).
    pending_end_tag: Option<&'a str>,
}

impl<'a> Tokenizer<'a> {
    /// Create a new tokenizer for the given HTML input.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
            pending_end_tag: None,
        }
    }

    /// Get the next token from the input.
    pub fn next_token(&mut self) -> Option<Token<'a>> {
        // Return pending end tag from raw text element handling.
        if let Some(name) = self.pending_end_tag.take() {
            return Some(Token::EndTag(name));
        }

        if self.pos >= self.bytes.len() {
            return None;
        }

        let remaining = &self.bytes[self.pos..];

        // Use SIMD to find the next '<'
        if let Some(lt_offset) = simd::find_lt(remaining) {
            if lt_offset > 0 {
                // There's text before the '<'
                let text = &self.input[self.pos..self.pos + lt_offset];
                self.pos += lt_offset;
                return Some(Token::Text(text));
            }

            // We're at a '<', parse the tag
            return self.parse_tag();
        }

        // No more '<', rest is text
        let text = &self.input[self.pos..];
        self.pos = self.bytes.len();
        if text.is_empty() {
            None
        } else {
            Some(Token::Text(text))
        }
    }

    /// Parse a tag starting at the current position (which should be '<').
    fn parse_tag(&mut self) -> Option<Token<'a>> {
        let start = self.pos;
        let remaining = &self.bytes[self.pos..];

        // Find the closing '>' (skip '>' inside quoted attribute values)
        let gt_offset = match find_tag_close(remaining) {
            Some(offset) => offset,
            None => {
                // Malformed: no closing '>', treat as text
                self.pos += 1;
                return Some(Token::Text(&self.input[start..start + 1]));
            }
        };

        let tag_content = &self.input[start + 1..start + gt_offset];
        self.pos = start + gt_offset + 1;

        // Check for comment
        if tag_content.starts_with("!--") {
            return self.parse_comment(start);
        }

        // Check for DOCTYPE
        if tag_content.len() >= 8 && tag_content[..8].eq_ignore_ascii_case("!doctype") {
            return Some(Token::Doctype(trim_ascii(&tag_content[8..])));
        }

        // Check for CDATA (treat as text)
        if tag_content.starts_with("![CDATA[") {
            return self.parse_cdata(start);
        }

        // Check for end tag
        if let Some(rest) = tag_content.strip_prefix('/') {
            let name = trim_ascii(rest);
            return Some(Token::EndTag(name));
        }

        // Check for self-closing
        let (content, is_self_closing) = if let Some(stripped) = tag_content.strip_suffix('/') {
            (stripped, true)
        } else {
            (tag_content, false)
        };

        // Parse the tag
        let tag = self.parse_tag_content(content);

        if is_self_closing || is_void_element(tag.name) {
            Some(Token::SelfClosingTag(tag))
        } else if is_raw_text_element(tag.name) {
            let name = tag.name;
            self.skip_raw_text_content(name);
            self.pending_end_tag = Some(name);
            Some(Token::StartTag(tag))
        } else {
            Some(Token::StartTag(tag))
        }
    }

    /// Parse comment content. Comments end with `-->`.
    fn parse_comment(&mut self, start: usize) -> Option<Token<'a>> {
        // We've already consumed past the initial `>` but comments need `-->`
        // Reset position and search properly
        self.pos = start + 4; // Skip `<!--`

        let remaining = &self.input[self.pos..];
        let bytes = remaining.as_bytes();
        let mut search_pos = 0;
        let end_idx = loop {
            let rel = match simd::find_char(&bytes[search_pos..], b'-') {
                Some(rel) => rel,
                None => break None,
            };
            let idx = search_pos + rel;
            if idx + 2 < bytes.len() && bytes[idx + 1] == b'-' && bytes[idx + 2] == b'>' {
                break Some(idx);
            }
            search_pos = idx + 1;
        };

        if let Some(end_idx) = end_idx {
            let comment = &remaining[..end_idx];
            self.pos += end_idx + 3;
            Some(Token::Comment(comment))
        } else {
            // Unclosed comment, consume rest
            let comment = remaining;
            self.pos = self.bytes.len();
            Some(Token::Comment(comment))
        }
    }

    /// Parse CDATA section.
    fn parse_cdata(&mut self, start: usize) -> Option<Token<'a>> {
        self.pos = start + 9; // Skip `<![CDATA[`

        let remaining = &self.input[self.pos..];
        let bytes = remaining.as_bytes();
        let mut search_pos = 0;
        let end_idx = loop {
            let rel = match simd::find_char(&bytes[search_pos..], b']') {
                Some(rel) => rel,
                None => break None,
            };
            let idx = search_pos + rel;
            if idx + 2 < bytes.len() && bytes[idx + 1] == b']' && bytes[idx + 2] == b'>' {
                break Some(idx);
            }
            search_pos = idx + 1;
        };

        if let Some(end_idx) = end_idx {
            let content = &remaining[..end_idx];
            self.pos += end_idx + 3;
            Some(Token::Text(content))
        } else {
            // Unclosed CDATA, consume rest
            let content = remaining;
            self.pos = self.bytes.len();
            Some(Token::Text(content))
        }
    }

    /// Skip content of a raw text element (script, style, noscript).
    /// Scans forward for the matching `</tagname>` and advances past it.
    fn skip_raw_text_content(&mut self, tag_name: &str) {
        let tag_name_len = tag_name.len();

        loop {
            let remaining = &self.bytes[self.pos..];
            let lt_offset = match simd::find_lt(remaining) {
                Some(offset) => offset,
                None => {
                    self.pos = self.bytes.len();
                    return;
                }
            };

            let lt_pos = self.pos + lt_offset;

            // Check for </tagname> (case-insensitive)
            let name_start = lt_pos + 2;
            let name_end = name_start + tag_name_len;

            if name_end <= self.bytes.len()
                && self.bytes[lt_pos + 1] == b'/'
                && self.input[name_start..name_end].eq_ignore_ascii_case(tag_name)
            {
                // Character after tag name must be '>' or whitespace
                if name_end == self.bytes.len()
                    || self.bytes[name_end] == b'>'
                    || self.bytes[name_end].is_ascii_whitespace()
                {
                    // Found the closing tag — advance past '>'
                    if let Some(gt_rel) = simd::find_gt(&self.bytes[lt_pos..]) {
                        self.pos = lt_pos + gt_rel + 1;
                    } else {
                        self.pos = self.bytes.len();
                    }
                    return;
                }
            }

            self.pos = lt_pos + 1;
        }
    }

    /// Parse the content of a tag into name and attributes.
    fn parse_tag_content(&self, content: &'a str) -> Tag<'a> {
        let content = trim_ascii(content);
        if content.is_empty() {
            return Tag {
                name: "",
                attributes: Vec::new(),
            };
        }

        // Find the tag name (first word)
        let name_end =
            simd::find_any_index(content.as_bytes(), TAG_NAME_DELIMS).unwrap_or(content.len());

        let name = &content[..name_end];
        let attrs_str = trim_ascii(&content[name_end..]);

        let attributes = if attrs_str.is_empty() {
            Vec::new()
        } else {
            parse_attributes(attrs_str)
        };

        Tag { name, attributes }
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

fn trim_ascii(input: &str) -> &str {
    let bytes = input.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();

    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    &input[start..end]
}

fn trim_start_ascii(input: &str) -> &str {
    let bytes = input.as_bytes();
    let mut start = 0;

    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }

    &input[start..]
}

/// Parse HTML attributes from a string.
fn parse_attributes(input: &str) -> Vec<Attribute<'_>> {
    let mut attrs = Vec::new();
    let mut remaining = trim_ascii(input);

    while !remaining.is_empty() {
        // Skip whitespace
        remaining = trim_start_ascii(remaining);
        if remaining.is_empty() {
            break;
        }

        // Find attribute name
        let name_end =
            simd::find_any_index(remaining.as_bytes(), ATTR_NAME_DELIMS).unwrap_or(remaining.len());

        if name_end == 0 {
            // Skip invalid character
            remaining = &remaining[1..];
            continue;
        }

        let name = &remaining[..name_end];
        remaining = trim_start_ascii(&remaining[name_end..]);

        // Check for value
        if remaining.starts_with('=') {
            remaining = trim_start_ascii(&remaining[1..]);

            let value = if remaining.starts_with('"') {
                // Double-quoted value
                remaining = &remaining[1..];
                if let Some(end) = simd::find_char(remaining.as_bytes(), b'"') {
                    let val = &remaining[..end];
                    remaining = &remaining[end + 1..];
                    Some(val)
                } else {
                    // Unclosed quote, take rest
                    let val = remaining;
                    remaining = "";
                    Some(val)
                }
            } else if remaining.starts_with('\'') {
                // Single-quoted value
                remaining = &remaining[1..];
                if let Some(end) = simd::find_char(remaining.as_bytes(), b'\'') {
                    let val = &remaining[..end];
                    remaining = &remaining[end + 1..];
                    Some(val)
                } else {
                    let val = remaining;
                    remaining = "";
                    Some(val)
                }
            } else {
                // Unquoted value
                let end = simd::find_any_index(remaining.as_bytes(), ATTR_VALUE_DELIMS)
                    .unwrap_or(remaining.len());
                let val = &remaining[..end];
                remaining = &remaining[end..];
                if val.is_empty() { None } else { Some(val) }
            };

            attrs.push(Attribute { name, value });
        } else {
            // Boolean attribute (no value)
            attrs.push(Attribute { name, value: None });
        }
    }

    attrs
}

/// Check if an element is a void element (self-closing by default).
fn is_void_element(name: &str) -> bool {
    name.eq_ignore_ascii_case("area")
        || name.eq_ignore_ascii_case("base")
        || name.eq_ignore_ascii_case("br")
        || name.eq_ignore_ascii_case("col")
        || name.eq_ignore_ascii_case("embed")
        || name.eq_ignore_ascii_case("hr")
        || name.eq_ignore_ascii_case("img")
        || name.eq_ignore_ascii_case("input")
        || name.eq_ignore_ascii_case("link")
        || name.eq_ignore_ascii_case("meta")
        || name.eq_ignore_ascii_case("param")
        || name.eq_ignore_ascii_case("source")
        || name.eq_ignore_ascii_case("track")
        || name.eq_ignore_ascii_case("wbr")
}

/// Find the closing `>` of a tag, skipping `>` inside quoted attribute values.
///
/// Tailwind CSS classes like `[&amp;>span]:px-6` contain literal `>` characters
/// inside quoted attribute values. This function respects quoted strings so that
/// only `>` outside quotes is treated as the tag close.
fn find_tag_close(bytes: &[u8]) -> Option<usize> {
    let mut pos = 0;
    loop {
        let remaining = &bytes[pos..];
        match simd::find_any_index(remaining, b">\"'") {
            None => return None,
            Some(offset) => {
                let abs = pos + offset;
                match bytes[abs] {
                    b'>' => return Some(abs),
                    b'"' => {
                        let after = abs + 1;
                        if let Some(end) = simd::find_char(&bytes[after..], b'"') {
                            pos = after + end + 1;
                        } else {
                            return None;
                        }
                    }
                    b'\'' => {
                        let after = abs + 1;
                        if let Some(end) = simd::find_char(&bytes[after..], b'\'') {
                            pos = after + end + 1;
                        } else {
                            return None;
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

/// Check if an element is a raw text element (content is not parsed as HTML).
fn is_raw_text_element(name: &str) -> bool {
    name.eq_ignore_ascii_case("script")
        || name.eq_ignore_ascii_case("style")
        || name.eq_ignore_ascii_case("noscript")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tag() {
        let mut tok = Tokenizer::new("<div>");
        let token = tok.next().unwrap();
        assert!(matches!(token, Token::StartTag(tag) if tag.name == "div"));
        assert!(tok.next().is_none());
    }

    #[test]
    fn test_tag_with_attributes() {
        let mut tok = Tokenizer::new(r#"<a href="https://example.com" class="link">"#);
        let token = tok.next().unwrap();
        if let Token::StartTag(tag) = token {
            assert_eq!(tag.name, "a");
            assert_eq!(tag.get_attr("href"), Some("https://example.com"));
            assert_eq!(tag.get_attr("class"), Some("link"));
        } else {
            panic!("Expected StartTag");
        }
    }

    #[test]
    fn test_self_closing_tag() {
        let mut tok = Tokenizer::new("<br/>");
        let token = tok.next().unwrap();
        assert!(matches!(token, Token::SelfClosingTag(tag) if tag.name == "br"));
    }

    #[test]
    fn test_void_element() {
        let mut tok = Tokenizer::new("<br>");
        let token = tok.next().unwrap();
        assert!(matches!(token, Token::SelfClosingTag(tag) if tag.name == "br"));
    }

    #[test]
    fn test_end_tag() {
        let mut tok = Tokenizer::new("</div>");
        let token = tok.next().unwrap();
        assert!(matches!(token, Token::EndTag("div")));
    }

    #[test]
    fn test_text() {
        let mut tok = Tokenizer::new("Hello world");
        let token = tok.next().unwrap();
        assert!(matches!(token, Token::Text("Hello world")));
    }

    #[test]
    fn test_mixed_content() {
        let tok = Tokenizer::new("<p>Hello <strong>world</strong>!</p>");
        let tokens: Vec<_> = tok.collect();
        assert_eq!(tokens.len(), 7);
        assert!(matches!(&tokens[0], Token::StartTag(tag) if tag.name == "p"));
        assert!(matches!(&tokens[1], Token::Text("Hello ")));
        assert!(matches!(&tokens[2], Token::StartTag(tag) if tag.name == "strong"));
        assert!(matches!(&tokens[3], Token::Text("world")));
        assert!(matches!(&tokens[4], Token::EndTag("strong")));
        assert!(matches!(&tokens[5], Token::Text("!")));
        assert!(matches!(&tokens[6], Token::EndTag("p")));
    }

    #[test]
    fn test_comment() {
        let mut tok = Tokenizer::new("<!-- this is a comment -->");
        let token = tok.next().unwrap();
        assert!(matches!(token, Token::Comment(" this is a comment ")));
    }

    #[test]
    fn test_doctype() {
        let mut tok = Tokenizer::new("<!DOCTYPE html>");
        let token = tok.next().unwrap();
        assert!(matches!(token, Token::Doctype("html")));
    }

    #[test]
    fn test_img_tag() {
        let mut tok = Tokenizer::new(r#"<img src="image.png" alt="An image">"#);
        let token = tok.next().unwrap();
        if let Token::SelfClosingTag(tag) = token {
            assert_eq!(tag.name, "img");
            assert_eq!(tag.get_attr("src"), Some("image.png"));
            assert_eq!(tag.get_attr("alt"), Some("An image"));
        } else {
            panic!("Expected SelfClosingTag");
        }
    }

    #[test]
    fn test_boolean_attribute() {
        let mut tok = Tokenizer::new("<input disabled>");
        let token = tok.next().unwrap();
        if let Token::SelfClosingTag(tag) = token {
            assert_eq!(tag.name, "input");
            assert!(tag.attributes.iter().any(|a| a.name == "disabled"));
        } else {
            panic!("Expected SelfClosingTag");
        }
    }

    #[test]
    fn test_single_quoted_attribute() {
        let mut tok = Tokenizer::new("<div class='test'>");
        let token = tok.next().unwrap();
        if let Token::StartTag(tag) = token {
            assert_eq!(tag.get_attr("class"), Some("test"));
        } else {
            panic!("Expected StartTag");
        }
    }

    #[test]
    fn test_unquoted_attribute() {
        let mut tok = Tokenizer::new("<div class=test>");
        let token = tok.next().unwrap();
        if let Token::StartTag(tag) = token {
            assert_eq!(tag.get_attr("class"), Some("test"));
        } else {
            panic!("Expected StartTag");
        }
    }

    #[test]
    fn test_malformed_unclosed_tag() {
        let mut tok = Tokenizer::new("<div");
        let token = tok.next().unwrap();
        // Should treat the lone '<' as text
        assert!(matches!(token, Token::Text("<")));
    }

    #[test]
    fn test_gt_in_quoted_attribute() {
        // Tailwind CSS classes with > inside quoted attribute values
        let html = r#"<button class="[&amp;>span]:px-6 text-label">Click</button>"#;
        let tokens: Vec<_> = Tokenizer::new(html).collect();
        assert!(matches!(&tokens[0], Token::StartTag(tag) if tag.name == "button"));
        if let Token::StartTag(tag) = &tokens[0] {
            assert_eq!(tag.get_attr("class"), Some("[&amp;>span]:px-6 text-label"));
        }
        assert!(matches!(&tokens[1], Token::Text("Click")));
        assert!(matches!(&tokens[2], Token::EndTag("button")));
    }

    #[test]
    fn test_gt_in_single_quoted_attribute() {
        let html = "<div class='a>b'>text</div>";
        let tokens: Vec<_> = Tokenizer::new(html).collect();
        assert!(matches!(&tokens[0], Token::StartTag(tag) if tag.name == "div"));
        if let Token::StartTag(tag) = &tokens[0] {
            assert_eq!(tag.get_attr("class"), Some("a>b"));
        }
        assert!(matches!(&tokens[1], Token::Text("text")));
    }

    #[test]
    fn test_multiple_gt_in_attributes() {
        let html = r#"<a href="x" class="[&amp;>*]:rel [&amp;>div]:flex">Link</a>"#;
        let tokens: Vec<_> = Tokenizer::new(html).collect();
        assert!(matches!(&tokens[0], Token::StartTag(tag) if tag.name == "a"));
        if let Token::StartTag(tag) = &tokens[0] {
            assert_eq!(tag.get_attr("href"), Some("x"));
            assert_eq!(
                tag.get_attr("class"),
                Some("[&amp;>*]:rel [&amp;>div]:flex")
            );
        }
        assert!(matches!(&tokens[1], Token::Text("Link")));
    }
}
