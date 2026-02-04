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
}

impl<'a> Tokenizer<'a> {
    /// Create a new tokenizer for the given HTML input.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    /// Get the next token from the input.
    pub fn next_token(&mut self) -> Option<Token<'a>> {
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

        // Find the closing '>'
        let gt_offset = match simd::find_gt(remaining) {
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
        if tag_content.len() >= 8
            && tag_content[..8].eq_ignore_ascii_case("!doctype")
        {
            return Some(Token::Doctype(tag_content[8..].trim()));
        }

        // Check for CDATA (treat as text)
        if tag_content.starts_with("![CDATA[") {
            return self.parse_cdata(start);
        }

        // Check for end tag
        if tag_content.starts_with('/') {
            let name = tag_content[1..].trim();
            return Some(Token::EndTag(name));
        }

        // Check for self-closing
        let (content, is_self_closing) = if tag_content.ends_with('/') {
            (&tag_content[..tag_content.len() - 1], true)
        } else {
            (tag_content, false)
        };

        // Parse the tag
        let tag = self.parse_tag_content(content);

        if is_self_closing || is_void_element(tag.name) {
            Some(Token::SelfClosingTag(tag))
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
        if let Some(end_idx) = remaining.find("-->") {
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
        if let Some(end_idx) = remaining.find("]]>") {
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

    /// Parse the content of a tag into name and attributes.
    fn parse_tag_content(&self, content: &'a str) -> Tag<'a> {
        let content = content.trim();
        if content.is_empty() {
            return Tag {
                name: "",
                attributes: Vec::new(),
            };
        }

        // Find the tag name (first word)
        let name_end = content
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(content.len());

        let name = &content[..name_end];
        let attrs_str = content[name_end..].trim();

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

/// Parse HTML attributes from a string.
fn parse_attributes(input: &str) -> Vec<Attribute<'_>> {
    let mut attrs = Vec::new();
    let mut remaining = input.trim();

    while !remaining.is_empty() {
        // Skip whitespace
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Find attribute name
        let name_end = remaining
            .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/' || c == '>')
            .unwrap_or(remaining.len());

        if name_end == 0 {
            // Skip invalid character
            remaining = &remaining[1..];
            continue;
        }

        let name = &remaining[..name_end];
        remaining = &remaining[name_end..].trim_start();

        // Check for value
        if remaining.starts_with('=') {
            remaining = &remaining[1..].trim_start();

            let value = if remaining.starts_with('"') {
                // Double-quoted value
                remaining = &remaining[1..];
                if let Some(end) = remaining.find('"') {
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
                if let Some(end) = remaining.find('\'') {
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
                let end = remaining
                    .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
                    .unwrap_or(remaining.len());
                let val = &remaining[..end];
                remaining = &remaining[end..];
                if val.is_empty() {
                    None
                } else {
                    Some(val)
                }
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
    matches!(
        name.to_ascii_lowercase().as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
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
}
