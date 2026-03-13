//! Main HTML to Markdown conversion logic with state machine.

use crate::emitter::{
    Alignment, TableFormatter, clean_text_cow, escape_markdown_cow, format_br, format_code,
    format_code_block, format_heading, format_hr, format_image, format_link, format_ordered_item,
    format_unordered_item,
};
use crate::entities::decode_entities_cow;
use crate::simd;
use crate::tokenizer::{Tag, Token, Tokenizer};
use std::borrow::Cow;

/// Conversion options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Bullet character for unordered lists.
    pub bullet_char: char,
    /// Use fenced code blocks (```) vs indented.
    pub code_block_fenced: bool,
    /// Preserve line breaks within paragraphs.
    pub preserve_line_breaks: bool,
    /// Tag names whose content should be completely skipped during conversion.
    /// Default: `["script", "style", "noscript"]`.
    pub skip_tags: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bullet_char: '-',
            code_block_fenced: true,
            preserve_line_breaks: false,
            skip_tags: vec![
                "script".to_string(),
                "style".to_string(),
                "noscript".to_string(),
                "head".to_string(),
            ],
        }
    }
}

/// List type in the stack.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ListType {
    Ordered(usize), // Current item number
    Unordered,
}

/// Context for tracking state during conversion.
#[derive(Debug)]
struct Context<'a> {
    /// Stack of list types (for nesting).
    list_stack: Vec<ListType>,
    /// Blockquote depth.
    blockquote_depth: usize,
    /// Whether we're in a code block.
    in_code_block: bool,
    /// Whether we're in inline code.
    in_code: bool,
    /// Accumulated inline code content.
    inline_code_content: String,
    /// Whether we're in a preformatted section.
    in_pre: bool,
    /// Code block language (from class attribute).
    code_language: Option<&'a str>,
    /// Accumulated code block content.
    code_content: String,
    /// Stack of inline formatting.
    inline_stack: Vec<InlineState>,
    /// Link URL being built.
    link_url: Option<&'a str>,
    /// Link title being built.
    link_title: Option<&'a str>,
    /// Link text being accumulated.
    link_text: String,
    /// Whether we're in a link.
    in_link: bool,
    /// Table being built.
    table: Option<TableState>,
    /// Current output buffer.
    output: String,
    /// Pending text that needs to be processed.
    pending_text: String,
    /// Whether we just emitted a block element.
    just_emitted_block: bool,
    /// Whether we need a newline before next content.
    needs_newline: bool,
    /// Current heading level (0 if not in heading).
    heading_level: u8,
    /// Accumulated heading text.
    heading_text: String,
    /// Whether we're in a paragraph.
    in_paragraph: bool,
    /// Stack of list item content (for nested lists).
    list_item_stack: Vec<String>,
    /// Depth counter for skipping tag content (skip_tags feature).
    skip_depth: usize,
    /// Tag name being skipped (for matching end tags).
    skip_tag_name: &'a str,
    /// Depth counter for skipping gutter/line-number elements in code blocks.
    code_skip_depth: usize,
    /// Whether an inline format marker was just opened (for leading whitespace trimming).
    just_opened_inline: bool,
    /// Whether an inline format marker was just closed (for trailing whitespace addition).
    just_closed_inline: bool,
    /// Pending inline space: whitespace-only text was seen; emit a space before next content.
    pending_space: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InlineFormat {
    Strong,
    Emphasis,
    Strikethrough,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct InlineState {
    kind: InlineFormat,
    opened: bool,
    /// True if this is a duplicate nesting (e.g., <i> inside <em>).
    /// Nested entries don't emit markers but are tracked for proper close pairing.
    nested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TagKind {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    P,
    Strong,
    Emphasis,
    Strikethrough,
    Code,
    Pre,
    A,
    Ul,
    Ol,
    Li,
    Blockquote,
    Table,
    Thead,
    Tbody,
    Tfoot,
    Tr,
    Th,
    Td,
    Div,
    Span,
    Section,
    Article,
    Header,
    Footer,
    Main,
    Aside,
    Nav,
    Br,
    Hr,
    Img,
    Input,
    Unknown,
}

#[derive(Debug)]
struct TableState {
    in_header: bool,
    in_row: bool,
    in_cell: bool,
    current_row_is_header: bool,
    has_headers: bool,
    cell_content: String,
    current_row: Vec<String>,
    formatter: TableFormatter,
}

impl<'a> Context<'a> {
    fn with_capacity(output_capacity: usize) -> Self {
        Self {
            list_stack: Vec::new(),
            blockquote_depth: 0,
            in_code_block: false,
            in_code: false,
            in_pre: false,
            code_language: None,
            code_content: String::new(),
            inline_code_content: String::new(),
            inline_stack: Vec::new(),
            link_url: None,
            link_title: None,
            link_text: String::new(),
            in_link: false,
            table: None,
            output: String::with_capacity(output_capacity),
            pending_text: String::new(),
            just_emitted_block: true,
            needs_newline: false,
            heading_level: 0,
            heading_text: String::new(),
            in_paragraph: false,
            list_item_stack: Vec::new(),
            skip_depth: 0,
            skip_tag_name: "",
            code_skip_depth: 0,
            just_opened_inline: false,
            just_closed_inline: false,
            pending_space: false,
        }
    }

    /// Get current list item content mutably.
    fn current_list_item(&mut self) -> Option<&mut String> {
        self.list_item_stack.last_mut()
    }

    /// Ensure proper spacing before a block element.
    fn ensure_block_spacing(&mut self) {
        if !self.output.is_empty() && !self.just_emitted_block {
            if self.blockquote_depth > 0 && !self.in_code_block {
                // Inside blockquote: blank lines need > prefix
                if !self.ends_with_bq_blank_line() && !self.output.ends_with("\n\n") {
                    if !self.output.ends_with('\n') {
                        self.output.push('\n');
                    }
                    self.push_bq_blank_line();
                }
            } else if !self.output.ends_with("\n\n") {
                if self.output.ends_with('\n') {
                    self.output.push('\n');
                } else {
                    self.output.push_str("\n\n");
                }
            }
        }
    }

    /// Emit content with proper prefix for lists/blockquotes.
    fn emit(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        // Apply blockquote prefix if needed
        if self.blockquote_depth > 0 && !self.in_code_block {
            let mut first = true;
            for line in text.lines() {
                if !first {
                    self.output.push('\n');
                }
                first = false;

                for _ in 0..self.blockquote_depth {
                    self.output.push_str("> ");
                }
                self.output.push_str(line);
            }
        } else {
            self.output.push_str(text);
        }
        self.just_emitted_block = false;
    }

    /// Emit a block element.
    fn emit_block(&mut self, text: &str) {
        self.ensure_block_spacing();
        self.emit(text);
        if self.blockquote_depth > 0 && !self.in_code_block {
            if !self.output.ends_with('\n') {
                self.output.push('\n');
            }
            self.push_bq_blank_line();
        } else {
            self.output.push_str("\n\n");
        }
        self.just_emitted_block = true;
    }

    /// Push a blockquote-prefixed blank line (e.g. ">" for depth 1, "> >" for depth 2).
    fn push_bq_blank_line(&mut self) {
        for i in 0..self.blockquote_depth {
            if i > 0 {
                self.output.push(' ');
            }
            self.output.push('>');
        }
        self.output.push('\n');
    }

    /// Check if output ends with a blockquote blank line (\n{prefix}\n).
    fn ends_with_bq_blank_line(&self) -> bool {
        if self.blockquote_depth == 0 {
            return false;
        }
        let bytes = self.output.as_bytes();
        let prefix_len = self.blockquote_depth * 2 - 1; // "> > >" chars
        let total = 1 + prefix_len + 1; // \n + prefix + \n
        if bytes.len() < total {
            return false;
        }
        let start = bytes.len() - total;
        if bytes[start] != b'\n' || bytes[bytes.len() - 1] != b'\n' {
            return false;
        }
        let mut pos = start + 1;
        for i in 0..self.blockquote_depth {
            if i > 0 {
                if bytes[pos] != b' ' {
                    return false;
                }
                pos += 1;
            }
            if bytes[pos] != b'>' {
                return false;
            }
            pos += 1;
        }
        pos == bytes.len() - 1
    }

    /// Emit inline content.
    fn emit_inline(&mut self, text: &str) {
        if self.needs_newline {
            self.output.push('\n');
            self.needs_newline = false;
        }
        self.emit(text);
    }

    /// Get current list indent level based on parent marker widths.
    fn list_indent(&self) -> usize {
        if self.list_stack.len() <= 1 {
            return 0;
        }
        let mut indent = 0;
        // Sum up marker widths of all parent lists (not the current one)
        for list_type in &self.list_stack[..self.list_stack.len() - 1] {
            indent += match list_type {
                ListType::Ordered(num) => {
                    // Width of "{num}. " = digits of (num-1 or num) + 2
                    // num is the next number to emit, so current item was num-1
                    let current = if *num > 1 { num - 1 } else { 1 };
                    let digits = if current == 0 {
                        1
                    } else {
                        (current as f64).log10() as usize + 1
                    };
                    digits + 2
                }
                ListType::Unordered => 2,
            };
        }
        indent
    }
}

/// Convert HTML to Markdown.
pub fn convert(html: &str, options: &Options) -> String {
    let mut ctx = Context::with_capacity(html.len());
    let tokenizer = Tokenizer::new(html);

    for token in tokenizer {
        process_token(&mut ctx, token, options);
    }

    // Finalize any pending content
    finalize(&mut ctx);

    // Clean up output
    let trimmed = ctx.output.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() == ctx.output.len() {
        ctx.output.push('\n');
        return ctx.output;
    }

    let mut output = trimmed.to_string();
    output.push('\n');
    output
}

/// Process a single token.
fn process_token<'a>(ctx: &mut Context<'a>, token: Token<'a>, options: &Options) {
    // Handle skip_tags mode: skip all content until matching end tag.
    if ctx.skip_depth > 0 {
        match &token {
            Token::StartTag(tag) if tag.name.eq_ignore_ascii_case(ctx.skip_tag_name) => {
                ctx.skip_depth += 1;
            }
            Token::EndTag(name) if name.eq_ignore_ascii_case(ctx.skip_tag_name) => {
                ctx.skip_depth -= 1;
            }
            _ => {}
        }
        return;
    }

    match token {
        Token::StartTag(tag) => {
            // Check if this tag should be skipped
            if should_skip_tag(tag.name, &options.skip_tags) {
                ctx.skip_depth = 1;
                ctx.skip_tag_name = tag.name;
                return;
            }
            process_start_tag(ctx, &tag, options);
        }
        Token::EndTag(name) => process_end_tag(ctx, name, options),
        Token::SelfClosingTag(tag) => process_self_closing_tag(ctx, &tag),
        Token::Text(text) => process_text(ctx, text),
        Token::Comment(_) => {} // Ignore comments
        Token::Doctype(_) => {} // Ignore doctype
    }
}

/// Process a start tag.
fn process_start_tag<'a>(ctx: &mut Context<'a>, tag: &Tag<'a>, options: &Options) {
    // Skip aria-hidden="true" elements globally (e.g., decorative shadow text).
    if tag
        .get_attr("aria-hidden")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        ctx.skip_depth = 1;
        ctx.skip_tag_name = tag.name;
        return;
    }

    let kind = tag_kind(tag.name);

    // Handle tags inside code blocks (except <code> for language detection).
    if ctx.in_code_block && kind != TagKind::Code {
        if ctx.code_skip_depth > 0 {
            ctx.code_skip_depth += 1;
            return;
        }
        // Skip gutter/line-number elements.
        if let Some(class) = tag.get_attr("class")
            && is_gutter_element(class)
        {
            ctx.code_skip_depth += 1;
            return;
        }
        // Skip aria-hidden elements (duplicated syntax-highlighted content).
        if tag
            .get_attr("aria-hidden")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
        {
            ctx.code_skip_depth += 1;
            return;
        }
        return;
    }

    match kind {
        // Headings
        TagKind::H1 => {
            ctx.heading_level = 1;
            ctx.heading_text.clear();
        }
        TagKind::H2 => {
            ctx.heading_level = 2;
            ctx.heading_text.clear();
        }
        TagKind::H3 => {
            ctx.heading_level = 3;
            ctx.heading_text.clear();
        }
        TagKind::H4 => {
            ctx.heading_level = 4;
            ctx.heading_text.clear();
        }
        TagKind::H5 => {
            ctx.heading_level = 5;
            ctx.heading_text.clear();
        }
        TagKind::H6 => {
            ctx.heading_level = 6;
            ctx.heading_text.clear();
        }

        // Paragraph
        TagKind::P => {
            // Inside a table cell, treat <p> as a separator (not a paragraph)
            if ctx.table.as_ref().is_some_and(|t| t.in_cell) {
                return;
            }
            // Inside a list item: add paragraph break before this <p> content
            if !ctx.list_item_stack.is_empty() {
                if let Some(content) = ctx.list_item_stack.last()
                    && !content.trim().is_empty()
                {
                    let content_ref = ctx.list_item_stack.last_mut().unwrap();
                    content_ref.push_str("\n\n");
                }
                ctx.in_paragraph = true;
                ctx.pending_text.clear();
                return;
            }
            // Auto-close current paragraph if already in one (malformed HTML like <p>...<p>)
            if ctx.in_paragraph {
                let pending = std::mem::take(&mut ctx.pending_text);
                let text = clean_text_cow(&pending);
                if !text.is_empty() {
                    let escaped = escape_leading_list_marker(text.as_ref());
                    ctx.emit_block(&escaped);
                }
            }
            ctx.in_paragraph = true;
            ctx.pending_text.clear();
        }

        // Emphasis
        TagKind::Strong => {
            let is_nested = ctx
                .inline_stack
                .iter()
                .any(|s| s.kind == InlineFormat::Strong);
            ctx.inline_stack.push(InlineState {
                kind: InlineFormat::Strong,
                opened: false,
                nested: is_nested,
            });
        }
        TagKind::Emphasis => {
            let is_nested = ctx
                .inline_stack
                .iter()
                .any(|s| s.kind == InlineFormat::Emphasis);
            ctx.inline_stack.push(InlineState {
                kind: InlineFormat::Emphasis,
                opened: false,
                nested: is_nested,
            });
        }
        TagKind::Strikethrough => {
            let is_nested = ctx
                .inline_stack
                .iter()
                .any(|s| s.kind == InlineFormat::Strikethrough);
            ctx.inline_stack.push(InlineState {
                kind: InlineFormat::Strikethrough,
                opened: false,
                nested: is_nested,
            });
        }

        // Code
        TagKind::Code => {
            if ctx.in_pre {
                // Code tag language takes priority over pre tag.
                if let Some(lang) = extract_language(tag) {
                    ctx.code_language = Some(lang);
                }
            } else {
                ctx.in_code = true;
                ctx.inline_code_content.clear();
            }
        }

        // Preformatted / code blocks
        TagKind::Pre => {
            ctx.in_pre = true;
            ctx.in_code_block = true;
            ctx.code_content.clear();
            // Check <pre> class for language (fallback; <code> takes priority).
            ctx.code_language = extract_language(tag);
        }

        // Links
        TagKind::A => {
            // Skip anchors without href (named anchors like <a id="top">)
            if tag.get_attr("href").is_none() {
                return;
            }
            if !ctx.in_link && !ctx.inline_stack.is_empty() {
                open_inline_markers(ctx);
            }
            ctx.in_link = true;
            ctx.link_url = tag.get_attr("href");
            ctx.link_title = tag.get_attr("title");
            ctx.link_text.clear();
        }

        // Lists
        TagKind::Ul => {
            // If we're in a list item, emit the current content before starting nested list
            if !ctx.list_item_stack.is_empty() {
                flush_current_list_item(ctx, options);
            }
            ctx.list_stack.push(ListType::Unordered);
        }
        TagKind::Ol => {
            // If we're in a list item, emit the current content before starting nested list
            if !ctx.list_item_stack.is_empty() {
                flush_current_list_item(ctx, options);
            }
            let start = tag
                .get_attr("start")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            ctx.list_stack.push(ListType::Ordered(start));
        }
        TagKind::Li => {
            ctx.list_item_stack.push(String::new());
        }

        // Blockquote
        TagKind::Blockquote => {
            ctx.blockquote_depth += 1;
        }

        // Table
        TagKind::Table => {
            ctx.table = Some(TableState {
                in_header: false,
                in_row: false,
                in_cell: false,
                current_row_is_header: false,
                has_headers: false,
                cell_content: String::new(),
                current_row: Vec::new(),
                formatter: TableFormatter::new(),
            });
        }
        TagKind::Thead => {
            if let Some(ref mut table) = ctx.table {
                table.in_header = true;
                table.has_headers = true;
            }
        }
        TagKind::Tbody | TagKind::Tfoot => {
            if let Some(ref mut table) = ctx.table {
                table.in_header = false;
            }
        }
        TagKind::Tr => {
            if let Some(ref mut table) = ctx.table {
                table.in_row = true;
                table.current_row.clear();
                table.current_row_is_header = false;
            }
        }
        TagKind::Th | TagKind::Td => {
            if let Some(ref mut table) = ctx.table
                && matches!(kind, TagKind::Th)
            {
                table.current_row_is_header = true;
            }
            if let Some(ref mut table) = ctx.table {
                table.in_cell = true;
                table.cell_content.clear();

                // Check for alignment
                if let Some(align) = tag.get_attr("align") {
                    let col = table.current_row.len();
                    let alignment = if align.eq_ignore_ascii_case("center") {
                        Alignment::Center
                    } else if align.eq_ignore_ascii_case("right") {
                        Alignment::Right
                    } else {
                        Alignment::Left
                    };
                    table.formatter.set_alignment(col, alignment);
                }
            }
        }

        // Block containers: ensure content separation at boundaries
        TagKind::Div
        | TagKind::Section
        | TagKind::Article
        | TagKind::Header
        | TagKind::Footer
        | TagKind::Main
        | TagKind::Aside
        | TagKind::Nav => {
            ensure_block_boundary(ctx);
        }

        // Span is inline, no special handling
        TagKind::Span => {}

        _ => {
            // Unknown tag, ignore
        }
    }
}

/// Process an end tag.
fn process_end_tag<'a>(ctx: &mut Context<'a>, name: &str, options: &Options) {
    let kind = tag_kind(name);

    // Handle end tags inside code blocks.
    if ctx.in_code_block && kind != TagKind::Code && kind != TagKind::Pre {
        if ctx.code_skip_depth > 0 {
            ctx.code_skip_depth -= 1;
            return;
        }
        // Block elements emit a newline to preserve line structure.
        if is_code_block_element(kind) && !ctx.code_content.ends_with('\n') {
            ctx.code_content.push('\n');
        }
        return;
    }

    match kind {
        // Headings
        TagKind::H1 | TagKind::H2 | TagKind::H3 | TagKind::H4 | TagKind::H5 | TagKind::H6
            if ctx.heading_level > 0 =>
        {
            let mut heading_text = std::mem::take(&mut ctx.heading_text);
            let text = clean_text_cow(&heading_text);
            if !text.is_empty() {
                if ctx.in_link {
                    let raw_url = ctx.link_url.take().unwrap_or("");
                    let title = ctx.link_title.take();
                    ctx.in_link = false;
                    ctx.link_text.clear();
                    // Decode HTML entities in URL
                    let decoded_url;
                    let url = if raw_url.contains('&') {
                        decoded_url = crate::entities::decode_entities(raw_url);
                        decoded_url.as_str()
                    } else {
                        raw_url
                    };
                    if url.starts_with('#') || url.is_empty() {
                        // Anchor/fragment link: drop it, just keep heading text
                        let heading = format_heading(ctx.heading_level, text.as_ref());
                        ctx.emit_block(&heading);
                    } else {
                        // Real link: preserve it in the heading
                        let link = format_link(text.as_ref(), url, title);
                        let heading = format_heading(ctx.heading_level, &link);
                        ctx.emit_block(&heading);
                    }
                } else {
                    let heading = format_heading(ctx.heading_level, text.as_ref());
                    ctx.emit_block(&heading);
                }
            }
            heading_text.clear();
            ctx.heading_text = heading_text;
            ctx.heading_level = 0;
        }

        // Paragraph
        TagKind::P => {
            // Inside a link, paragraph breaks become line breaks
            if ctx.in_link {
                if !ctx.link_text.is_empty() {
                    ctx.link_text.push_str("\\\n");
                }
                return;
            }
            // Inside a table cell, emit <br> as separator
            if let Some(ref mut table) = ctx.table
                && table.in_cell
            {
                if !table.cell_content.is_empty() {
                    table.cell_content.push_str("<br>");
                }
                return;
            }
            // Inside a list item: just close paragraph state; the separator
            // was already added when <p> opened.
            if !ctx.list_item_stack.is_empty() {
                ctx.in_paragraph = false;
                return;
            }
            if ctx.in_paragraph {
                let mut pending = std::mem::take(&mut ctx.pending_text);
                let text = clean_text_cow(&pending);
                if !text.is_empty() {
                    let escaped = escape_leading_list_marker(text.as_ref());
                    ctx.emit_block(&escaped);
                }
                pending.clear();
                ctx.pending_text = pending;
                ctx.in_paragraph = false;
            }
        }

        // Emphasis
        TagKind::Strong => {
            close_inline_format(ctx, InlineFormat::Strong);
        }
        TagKind::Emphasis => {
            close_inline_format(ctx, InlineFormat::Emphasis);
        }
        TagKind::Strikethrough => {
            close_inline_format(ctx, InlineFormat::Strikethrough);
        }

        // Code
        TagKind::Code if !ctx.in_pre => {
            if ctx.in_code {
                let code = format_code(&ctx.inline_code_content);
                emit_text_to_context(ctx, Cow::Owned(code), false);
                ctx.inline_code_content.clear();
            }
            ctx.in_code = false;
        }

        // Preformatted / code blocks
        TagKind::Pre if ctx.in_code_block => {
            let code = format_code_block(&ctx.code_content, ctx.code_language);
            // Must clear code block state before emit_block so blockquote
            // prefix is applied to the formatted code block.
            ctx.in_code_block = false;
            ctx.in_pre = false;
            ctx.emit_block(&code);
            ctx.code_content.clear();
            ctx.code_language = None;
            ctx.code_skip_depth = 0;
        }

        // Links
        TagKind::A if ctx.in_link => {
            let raw_url = ctx.link_url.take().unwrap_or("");
            let title = ctx.link_title.take();

            // Decode HTML entities in URL (e.g., &amp; → &)
            let decoded_url;
            let url = if raw_url.contains('&') {
                decoded_url = crate::entities::decode_entities(raw_url);
                decoded_url.as_str()
            } else {
                raw_url
            };

            // Remove "skip to content" navigation links.
            if is_skip_to_content_link(url, &ctx.link_text) {
                ctx.in_link = false;
                ctx.link_text.clear();
                return;
            }

            // Decode entities and normalize whitespace in title
            let decoded_title;
            let title = match title {
                Some(t) => {
                    let needs_decode = t.contains('&');
                    let needs_normalize = t.contains('\n') || t.contains('\r');
                    if needs_decode || needs_normalize {
                        let mut s = if needs_decode {
                            crate::entities::decode_entities(t)
                        } else {
                            t.to_string()
                        };
                        if needs_normalize {
                            s = s.split_whitespace().collect::<Vec<_>>().join(" ");
                        }
                        decoded_title = s;
                        Some(decoded_title.as_str())
                    } else {
                        Some(t)
                    }
                }
                None => None,
            };

            // Check if link wraps a single image and can be simplified
            let link_text_trimmed = ctx.link_text.trim();
            let is_image_only = link_text_trimmed.starts_with("![")
                && link_text_trimmed.ends_with(')')
                && link_text_trimmed.matches("![").count() == 1;

            let link = if is_image_only
                && (url.is_empty()
                    || (title.is_none() && link_text_trimmed.contains(&format!("]({})", url))))
            {
                // Link wraps an image with same/empty URL — just output the image
                link_text_trimmed.to_string()
            } else {
                // Clean link text, preserving backslash-newline sequences from <br>
                let text = if ctx.link_text.contains("\\\n") {
                    let lines: Vec<&str> = ctx.link_text.split("\\\n").collect();
                    let cleaned: Vec<String> = lines
                        .iter()
                        .map(|l| clean_text_cow(l).into_owned())
                        .collect();
                    Cow::Owned(cleaned.join("\\\n"))
                } else {
                    clean_text_cow(&ctx.link_text)
                };
                let link_text = if text.is_empty() {
                    title.unwrap_or(url)
                } else {
                    text.as_ref()
                };
                format_link(link_text, url, title)
            };

            // Inside a heading: drop anchor links, keep real links
            if ctx.heading_level > 0 {
                if url.starts_with('#') || url.is_empty() {
                    // Anchor link: drop it, text already in heading_text
                    ctx.in_link = false;
                    ctx.link_text.clear();
                    return;
                }
                // Real link: replace raw text in heading_text with formatted link
                let raw_len = ctx.link_text.len();
                let heading_len = ctx.heading_text.len();
                ctx.heading_text
                    .truncate(heading_len.saturating_sub(raw_len));
                ctx.heading_text.push_str(&link);
                ctx.in_link = false;
                ctx.link_text.clear();
                return;
            }

            ctx.in_link = false;
            ctx.link_text.clear();

            // Add space before link if preceded by non-whitespace
            if let Some(ch) = last_char_in_buffer(ctx)
                && !ch.is_whitespace()
                && ch != '\n'
            {
                append_to_context(ctx, " ");
            }

            // Emit the link
            emit_text_to_context(ctx, Cow::Owned(link), false);
            // Set flag so next text adds space after link if needed
            ctx.just_closed_inline = true;
        }

        // Lists
        TagKind::Ul | TagKind::Ol => {
            // Auto-close any open <li> before closing the list
            if !ctx.list_item_stack.is_empty() {
                let item_content = ctx.list_item_stack.pop().unwrap();
                let content = clean_text_cow(&item_content);
                if !ctx.list_stack.is_empty() {
                    if content.is_empty() {
                        if let Some(ListType::Ordered(num)) = ctx.list_stack.last_mut() {
                            *num += 1;
                        }
                    } else {
                        let indent = ctx.list_indent();
                        let item = match ctx.list_stack.last_mut() {
                            Some(ListType::Unordered) => {
                                format_unordered_item(content.as_ref(), indent, options.bullet_char)
                            }
                            Some(ListType::Ordered(num)) => {
                                let formatted = format_ordered_item(content.as_ref(), *num, indent);
                                *num += 1;
                                formatted
                            }
                            None => content.into_owned(),
                        };
                        if ctx.list_stack.len() == 1
                            && !ctx.output.is_empty()
                            && !ctx.output.ends_with('\n')
                        {
                            ctx.output.push('\n');
                        }
                        ctx.emit(&item);
                        ctx.output.push('\n');
                        ctx.just_emitted_block = false;
                    }
                }
            }
            ctx.list_stack.pop();
            if ctx.list_stack.is_empty() {
                // End of outermost list
                if !ctx.output.ends_with("\n\n") {
                    if ctx.output.ends_with('\n') {
                        ctx.output.push('\n');
                    } else {
                        ctx.output.push_str("\n\n");
                    }
                    ctx.just_emitted_block = true;
                }
            }
        }
        TagKind::Li => {
            if let Some(item_content) = ctx.list_item_stack.pop() {
                let content = if item_content.contains("\n\n") {
                    let paragraphs: Vec<&str> = item_content.split("\n\n").collect();
                    let cleaned: Vec<String> = paragraphs
                        .iter()
                        .map(|p| clean_text_cow(p).into_owned())
                        .filter(|c| !c.is_empty())
                        .collect();
                    Cow::Owned(cleaned.join("\n\n"))
                } else {
                    clean_text_cow(&item_content)
                };
                if !ctx.list_stack.is_empty() {
                    // Always increment ordered list counter (even for empty items)
                    if content.is_empty() {
                        if let Some(ListType::Ordered(num)) = ctx.list_stack.last_mut() {
                            *num += 1;
                        }
                    } else {
                        let indent = ctx.list_indent();
                        let item = match ctx.list_stack.last_mut() {
                            Some(ListType::Unordered) => {
                                format_unordered_item(content.as_ref(), indent, options.bullet_char)
                            }
                            Some(ListType::Ordered(num)) => {
                                let item = format_ordered_item(content.as_ref(), *num, indent);
                                *num += 1;
                                item
                            }
                            None => content.into_owned(),
                        };

                        // For first list item, ensure proper spacing from previous content
                        if ctx.list_stack.len() == 1
                            && !ctx.output.is_empty()
                            && !ctx.output.ends_with('\n')
                        {
                            ctx.output.push('\n');
                        }
                        ctx.emit(&item);
                        ctx.output.push('\n');
                        ctx.just_emitted_block = false;
                    }
                }
            }
        }

        // Blockquote
        TagKind::Blockquote if ctx.blockquote_depth > 0 => {
            // Clean up trailing blockquote blank line.
            // emit_block leaves \n{prefix}\n at the end; convert to the
            // outer depth's format (or plain \n\n for outermost close).
            if ctx.ends_with_bq_blank_line() {
                let prefix_len = ctx.blockquote_depth * 2 - 1;
                let remove = prefix_len + 1; // prefix + trailing \n
                ctx.output.truncate(ctx.output.len() - remove);
                ctx.blockquote_depth -= 1;
                if ctx.blockquote_depth > 0 {
                    ctx.push_bq_blank_line();
                } else {
                    ctx.output.push('\n');
                }
            } else {
                ctx.blockquote_depth -= 1;
            }
        }

        // Table
        TagKind::Table => {
            if let Some(mut table) = ctx.table.take() {
                // Auto-generate empty headers for tables without <th>
                if !table.has_headers {
                    let max_cols = table.formatter.max_columns();
                    if max_cols > 0 {
                        let empty_headers: Vec<String> =
                            (0..max_cols).map(|_| String::new()).collect();
                        table.formatter.set_headers(empty_headers);
                    }
                }
                let formatted = table.formatter.format();
                if !formatted.is_empty() {
                    ctx.emit_block(&formatted);
                }
            }
        }
        TagKind::Thead => {
            if let Some(ref mut table) = ctx.table {
                table.in_header = false;
            }
        }
        TagKind::Tr => {
            if let Some(ref mut table) = ctx.table
                && table.in_row
            {
                if table.in_header || (table.current_row_is_header && !table.has_headers) {
                    table.formatter.set_headers(table.current_row.clone());
                    table.has_headers = true;
                } else {
                    table.formatter.add_row(table.current_row.clone());
                }
                table.current_row.clear();
                table.in_row = false;
            }
        }
        TagKind::Th | TagKind::Td => {
            if let Some(ref mut table) = ctx.table
                && table.in_cell
            {
                let mut cleaned = clean_text_cow(&table.cell_content).into_owned();
                // Strip trailing <br> from cell content
                while cleaned.ends_with("<br>") {
                    cleaned.truncate(cleaned.len() - 4);
                    cleaned = cleaned.trim_end().to_string();
                }
                // Normalize spaces around <br>
                if cleaned.contains("<br>") {
                    cleaned = cleaned.replace(" <br>", "<br>").replace("<br> ", "<br>");
                }
                // Escape unescaped pipe characters inside table cells
                let escaped = if cleaned.contains('|') {
                    // Only escape pipes that aren't already escaped
                    let mut result = String::with_capacity(cleaned.len() + 4);
                    let bytes = cleaned.as_bytes();
                    for (i, &b) in bytes.iter().enumerate() {
                        if b == b'|' && (i == 0 || bytes[i - 1] != b'\\') {
                            result.push('\\');
                        }
                        result.push(b as char);
                    }
                    result
                } else {
                    cleaned
                };
                table.current_row.push(escaped);
                table.cell_content.clear();
                table.in_cell = false;
            }
        }

        // Block containers: ensure content separation at boundaries
        TagKind::Div
        | TagKind::Section
        | TagKind::Article
        | TagKind::Header
        | TagKind::Footer
        | TagKind::Main
        | TagKind::Aside
        | TagKind::Nav => {
            ensure_block_boundary(ctx);
        }

        _ => {}
    }
}

/// Process a self-closing tag.
fn process_self_closing_tag<'a>(ctx: &mut Context<'a>, tag: &Tag<'a>) {
    if ctx.code_skip_depth > 0 {
        return;
    }

    match tag_kind(tag.name) {
        // Line break
        TagKind::Br => {
            if ctx.in_code_block {
                ctx.code_content.push('\n');
            } else if ctx.in_link {
                ctx.link_text.push_str("\\\n");
            } else if let Some(ref mut table) = ctx.table
                && table.in_cell
            {
                table.cell_content.push_str("<br>");
                return;
            }
            // Note: the above returns handle their cases. Fall through for remaining.
            if !ctx.in_code_block && !ctx.in_link && ctx.table.as_ref().is_none_or(|t| !t.in_cell) {
                if let Some(content) = ctx.current_list_item() {
                    content.push_str(format_br());
                } else if ctx.in_paragraph {
                    // Close any open inline formatting before the break
                    let open_formats: Vec<InlineFormat> = ctx
                        .inline_stack
                        .iter()
                        .filter(|s| s.opened)
                        .map(|s| s.kind)
                        .collect();
                    for kind in open_formats.iter().rev() {
                        trim_trailing_whitespace_in_buffer(ctx);
                        emit_inline_marker(ctx, *kind);
                    }
                    // Mark all as not opened (will reopen after break)
                    for state in &mut ctx.inline_stack {
                        state.opened = false;
                    }
                    // Flush current pending text as a block and continue paragraph
                    let pending = std::mem::take(&mut ctx.pending_text);
                    let text = clean_text_cow(&pending);
                    if !text.is_empty() {
                        ctx.emit_block(text.as_ref());
                    }
                    // Stay in paragraph mode for text after the <br>
                } else if ctx.heading_level > 0 {
                    ctx.heading_text.push(' ');
                } else {
                    ctx.emit_inline(format_br());
                }
            }
        }

        // Horizontal rule
        TagKind::Hr => {
            ctx.emit_block(format_hr());
        }

        // Image
        TagKind::Img => {
            let raw_src = tag.get_attr("src").unwrap_or("");
            // Skip images with no src or empty src
            if raw_src.is_empty() {
                return;
            }
            let raw_alt = tag.get_attr("alt").unwrap_or("");
            // Decode HTML entities and normalize newlines in src
            let src_decoded;
            let src_normalized;
            let src = if raw_src.contains('&') || raw_src.contains('\n') || raw_src.contains('\r') {
                let s = if raw_src.contains('&') {
                    src_decoded = crate::entities::decode_entities(raw_src);
                    src_decoded.as_str()
                } else {
                    raw_src
                };
                if s.contains('\n') || s.contains('\r') {
                    src_normalized = s.split_whitespace().collect::<Vec<_>>().join("");
                    src_normalized.as_str()
                } else {
                    s
                }
            } else {
                raw_src
            };
            // Normalize newlines in alt text to spaces
            let alt = if raw_alt.contains('\n') || raw_alt.contains('\r') {
                raw_alt.split_whitespace().collect::<Vec<_>>().join(" ")
            } else {
                raw_alt.to_string()
            };
            let title = tag.get_attr("title");

            let image = format_image(&alt, src, title);
            emit_text_to_context(ctx, Cow::Owned(image), false);
        }

        // Input (for checkboxes in task lists)
        TagKind::Input if tag.get_attr("type") == Some("checkbox") => {
            let checked = tag
                .attributes
                .iter()
                .any(|a| a.name.eq_ignore_ascii_case("checked"));
            let checkbox = if checked { "[x] " } else { "[ ] " };
            emit_text_to_context(ctx, Cow::Borrowed(checkbox), false);
        }

        _ => {}
    }
}

/// Process text content.
fn process_text<'a>(ctx: &mut Context<'a>, text: &'a str) {
    // Skip text inside gutter elements in code blocks.
    if ctx.code_skip_depth > 0 {
        return;
    }

    if !ctx.in_code_block
        && !ctx.in_code
        && !ctx.in_paragraph
        && !ctx.in_link
        && ctx.heading_level == 0
        && ctx.list_item_stack.is_empty()
        && ctx.table.as_ref().is_none_or(|table| !table.in_cell)
        && is_ascii_whitespace_only(text)
    {
        // Only skip whitespace at block boundaries (output ends with newline or is empty).
        // Preserve inline whitespace between content (e.g., "text <!-- --> more text").
        if ctx.output.is_empty() || ctx.output.ends_with('\n') || ctx.just_emitted_block {
            return;
        }
        // Set pending space flag — the space will be emitted when next content arrives.
        // This avoids trailing spaces when a block boundary follows the whitespace.
        ctx.pending_space = true;
        return;
    }

    // Decode HTML entities
    // Handle code blocks specially - preserve all whitespace
    if ctx.in_code_block {
        if simd::find_amp(text.as_bytes()).is_none() {
            ctx.code_content.push_str(text);
        } else {
            let decoded = decode_entities_cow(text);
            ctx.code_content.push_str(decoded.as_ref());
        }
        return;
    }

    // Handle inline code - preserve text until closing tag
    if ctx.in_code {
        if simd::find_amp(text.as_bytes()).is_none() {
            ctx.inline_code_content.push_str(text);
        } else {
            let decoded = decode_entities_cow(text);
            ctx.inline_code_content.push_str(decoded.as_ref());
        }
        return;
    }

    // Table cell text goes through the normal emit_text_to_context path
    // so that inline formatting (bold, italic, links) works inside cells.

    let should_escape = ctx.inline_stack.is_empty() && !ctx.in_link;
    if should_escape && !contains_special_or_amp(text.as_bytes()) {
        emit_text_to_context(ctx, Cow::Borrowed(text), false);
        return;
    }

    let decoded = if simd::find_amp(text.as_bytes()).is_some() {
        decode_entities_cow(text)
    } else {
        Cow::Borrowed(text)
    };

    emit_text_to_context(ctx, decoded, should_escape);
}

fn is_ascii_whitespace_only(text: &str) -> bool {
    text.as_bytes().iter().all(|b| b.is_ascii_whitespace())
}

fn contains_special_or_amp(bytes: &[u8]) -> bool {
    const ESCAPE_SIMD_THRESHOLD: usize = 64;
    let special = b"\\`*_[]#|~&";

    if bytes.len() >= ESCAPE_SIMD_THRESHOLD {
        simd::find_any_index(bytes, special).is_some()
    } else {
        bytes.iter().any(|&b| {
            matches!(
                b,
                b'\\' | b'`' | b'*' | b'_' | b'[' | b']' | b'#' | b'|' | b'~' | b'&'
            )
        })
    }
}

fn tag_kind(name: &str) -> TagKind {
    let bytes = name.as_bytes();
    match bytes.len() {
        1 => match lower_ascii(bytes[0]) {
            b'a' => TagKind::A,
            b'p' => TagKind::P,
            b'b' => TagKind::Strong,
            b'i' => TagKind::Emphasis,
            b's' => TagKind::Strikethrough,
            _ => TagKind::Unknown,
        },
        2 => {
            let b0 = lower_ascii(bytes[0]);
            let b1 = lower_ascii(bytes[1]);
            match (b0, b1) {
                (b'h', b'1') => TagKind::H1,
                (b'h', b'2') => TagKind::H2,
                (b'h', b'3') => TagKind::H3,
                (b'h', b'4') => TagKind::H4,
                (b'h', b'5') => TagKind::H5,
                (b'h', b'6') => TagKind::H6,
                (b'e', b'm') => TagKind::Emphasis,
                (b'o', b'l') => TagKind::Ol,
                (b'u', b'l') => TagKind::Ul,
                (b'l', b'i') => TagKind::Li,
                (b'b', b'r') => TagKind::Br,
                (b'h', b'r') => TagKind::Hr,
                (b't', b'r') => TagKind::Tr,
                (b't', b'h') => TagKind::Th,
                (b't', b'd') => TagKind::Td,
                (b't', b't') => TagKind::Code,
                _ => TagKind::Unknown,
            }
        }
        3 => {
            let b0 = lower_ascii(bytes[0]);
            let b1 = lower_ascii(bytes[1]);
            let b2 = lower_ascii(bytes[2]);
            match (b0, b1, b2) {
                (b'd', b'i', b'v') => TagKind::Div,
                (b'p', b'r', b'e') => TagKind::Pre,
                (b'i', b'm', b'g') => TagKind::Img,
                (b'n', b'a', b'v') => TagKind::Nav,
                (b'd', b'e', b'l') => TagKind::Strikethrough,
                (b'k', b'b', b'd') => TagKind::Code,
                _ => TagKind::Unknown,
            }
        }
        4 => {
            let b0 = lower_ascii(bytes[0]);
            let b1 = lower_ascii(bytes[1]);
            let b2 = lower_ascii(bytes[2]);
            let b3 = lower_ascii(bytes[3]);
            match (b0, b1, b2, b3) {
                (b's', b'p', b'a', b'n') => TagKind::Span,
                (b'c', b'o', b'd', b'e') => TagKind::Code,
                (b'm', b'a', b'i', b'n') => TagKind::Main,
                (b's', b'a', b'm', b'p') => TagKind::Code,
                _ => TagKind::Unknown,
            }
        }
        5 => {
            let b0 = lower_ascii(bytes[0]);
            let b1 = lower_ascii(bytes[1]);
            let b2 = lower_ascii(bytes[2]);
            let b3 = lower_ascii(bytes[3]);
            let b4 = lower_ascii(bytes[4]);
            match (b0, b1, b2, b3, b4) {
                (b't', b'a', b'b', b'l', b'e') => TagKind::Table,
                (b't', b'h', b'e', b'a', b'd') => TagKind::Thead,
                (b't', b'b', b'o', b'd', b'y') => TagKind::Tbody,
                (b't', b'f', b'o', b'o', b't') => TagKind::Tfoot,
                (b'a', b's', b'i', b'd', b'e') => TagKind::Aside,
                (b'i', b'n', b'p', b'u', b't') => TagKind::Input,
                _ => TagKind::Unknown,
            }
        }
        6 => {
            let b0 = lower_ascii(bytes[0]);
            let b1 = lower_ascii(bytes[1]);
            let b2 = lower_ascii(bytes[2]);
            let b3 = lower_ascii(bytes[3]);
            let b4 = lower_ascii(bytes[4]);
            let b5 = lower_ascii(bytes[5]);
            match (b0, b1, b2, b3, b4, b5) {
                (b's', b't', b'r', b'o', b'n', b'g') => TagKind::Strong,
                (b's', b't', b'r', b'i', b'k', b'e') => TagKind::Strikethrough,
                (b'h', b'e', b'a', b'd', b'e', b'r') => TagKind::Header,
                (b'f', b'o', b'o', b't', b'e', b'r') => TagKind::Footer,
                _ => TagKind::Unknown,
            }
        }
        7 => {
            let b0 = lower_ascii(bytes[0]);
            let b1 = lower_ascii(bytes[1]);
            let b2 = lower_ascii(bytes[2]);
            let b3 = lower_ascii(bytes[3]);
            let b4 = lower_ascii(bytes[4]);
            let b5 = lower_ascii(bytes[5]);
            let b6 = lower_ascii(bytes[6]);
            match (b0, b1, b2, b3, b4, b5, b6) {
                (b's', b'e', b'c', b't', b'i', b'o', b'n') => TagKind::Section,
                (b'a', b'r', b't', b'i', b'c', b'l', b'e') => TagKind::Article,
                _ => TagKind::Unknown,
            }
        }
        10 => {
            if name.eq_ignore_ascii_case("blockquote") {
                TagKind::Blockquote
            } else {
                TagKind::Unknown
            }
        }
        _ => TagKind::Unknown,
    }
}

fn lower_ascii(byte: u8) -> u8 {
    if byte.is_ascii_uppercase() {
        byte + 32
    } else {
        byte
    }
}

/// Check if a tag name should be skipped based on the skip_tags list.
fn should_skip_tag(name: &str, skip_tags: &[String]) -> bool {
    skip_tags.iter().any(|t| name.eq_ignore_ascii_case(t))
}

/// Extract language identifier from a tag's class attribute.
/// Looks for `language-*` or `lang-*` prefixes.
fn extract_language<'a>(tag: &Tag<'a>) -> Option<&'a str> {
    if let Some(class) = tag.get_attr("class") {
        for part in class.split_whitespace() {
            if let Some(lang) = part.strip_prefix("language-") {
                return Some(lang);
            } else if let Some(lang) = part.strip_prefix("lang-") {
                return Some(lang);
            }
        }
    }
    None
}

/// Check if a class attribute indicates a gutter/line-number element.
fn is_gutter_element(class: &str) -> bool {
    let bytes = class.as_bytes();
    contains_ascii_ci(bytes, b"gutter") || contains_ascii_ci(bytes, b"line-number")
}

/// Case-insensitive ASCII substring search (needle must be lowercase).
fn contains_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(h, n)| h.eq_ignore_ascii_case(n))
    })
}

/// Check if a tag kind is a block element that should emit newlines in code blocks.
fn is_code_block_element(kind: TagKind) -> bool {
    matches!(
        kind,
        TagKind::Div
            | TagKind::P
            | TagKind::Tr
            | TagKind::Li
            | TagKind::Section
            | TagKind::Article
            | TagKind::Table
            | TagKind::Thead
            | TagKind::Tbody
            | TagKind::Tfoot
            | TagKind::H1
            | TagKind::H2
            | TagKind::H3
            | TagKind::H4
            | TagKind::H5
            | TagKind::H6
    )
}

/// Ensure a block boundary (newline) exists in the output when a block-level
/// container opens or closes.  This prevents text from adjacent block elements
/// being concatenated (e.g., `</div><div>` should not join words).
fn ensure_block_boundary(ctx: &mut Context) {
    // Clear any pending inline space — block boundary supersedes it
    ctx.pending_space = false;

    // Don't add boundaries inside special contexts that manage their own spacing
    if ctx.in_code_block || ctx.in_code || ctx.heading_level > 0 {
        return;
    }
    // Inside a paragraph, mark a break in pending_text
    if ctx.in_paragraph && !ctx.pending_text.is_empty() {
        if !ctx.pending_text.ends_with('\n') {
            ctx.pending_text.push('\n');
        }
        return;
    }
    // Inside a list item, add a break
    if let Some(content) = ctx.list_item_stack.last_mut() {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        return;
    }
    // Inside a link, add a space (newlines would break the link syntax)
    if ctx.in_link {
        if !ctx.link_text.is_empty() && !ctx.link_text.ends_with(' ') {
            ctx.link_text.push(' ');
        }
        return;
    }
    // Bare output: ensure newline
    if !ctx.output.is_empty() && !ctx.output.ends_with('\n') {
        ctx.output.push('\n');
        ctx.just_emitted_block = false;
    }
}

/// Get the last character in the current context buffer.
fn last_char_in_buffer(ctx: &Context) -> Option<char> {
    if ctx.heading_level > 0 {
        ctx.heading_text.chars().last()
    } else if ctx.in_link {
        ctx.link_text.chars().last()
    } else if let Some(content) = ctx.list_item_stack.last() {
        content.chars().last()
    } else if let Some(ref table) = ctx.table {
        if table.in_cell {
            return table.cell_content.chars().last();
        }
        ctx.pending_text.chars().last()
    } else if ctx.in_paragraph {
        ctx.pending_text.chars().last()
    } else {
        ctx.output.chars().last()
    }
}

/// Whether a character is an inline formatting marker.
fn is_inline_marker_char(c: char) -> bool {
    matches!(c, '*' | '~')
}

/// Check if we need a space before an inline formatting marker.
fn needs_space_before_inline(ctx: &Context) -> bool {
    if ctx.just_closed_inline {
        return true;
    }
    match last_char_in_buffer(ctx) {
        None => false,
        Some(c) => !c.is_whitespace() && !is_inline_marker_char(c),
    }
}

/// Whether a byte is punctuation that should not be preceded by a space.
#[inline]
fn is_punctuation(b: u8) -> bool {
    matches!(
        b,
        b'.' | b',' | b':' | b';' | b'!' | b'?' | b')' | b']' | b'\'' | b'"' | b'\xE2'
    )
    // 0xE2 = start of UTF-8 multi-byte sequences like \u{2019} (right single quote)
}

/// Trim trailing whitespace from the current context buffer.
fn trim_trailing_whitespace_in_buffer(ctx: &mut Context) {
    let buf = if ctx.heading_level > 0 {
        &mut ctx.heading_text
    } else if ctx.in_link {
        &mut ctx.link_text
    } else if let Some(content) = ctx.list_item_stack.last_mut() {
        content
    } else if ctx.table.as_ref().is_some_and(|t| t.in_cell) {
        if let Some(ref mut table) = ctx.table {
            &mut table.cell_content
        } else {
            return;
        }
    } else if ctx.in_paragraph {
        &mut ctx.pending_text
    } else {
        &mut ctx.output
    };

    let new_len = buf.trim_end().len();
    buf.truncate(new_len);
}

/// Check if a link is a "skip to content" navigation link.
fn is_skip_to_content_link(url: &str, text: &str) -> bool {
    if !url.starts_with('#') {
        return false;
    }
    let trimmed = text.trim();
    trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("skip to")
}

// Inline formatting is streamed by emitting markers on tag open/close.

/// Flush the current list item content (used before starting a nested list).
fn flush_current_list_item<'a>(ctx: &mut Context<'a>, options: &Options) {
    if let Some(mut item_content) = ctx.list_item_stack.pop() {
        // For multi-paragraph list items, clean each paragraph separately
        // to preserve the paragraph breaks.
        let content = if item_content.contains("\n\n") {
            let paragraphs: Vec<&str> = item_content.split("\n\n").collect();
            let cleaned: Vec<String> = paragraphs
                .iter()
                .map(|p| {
                    let c = clean_text_cow(p);
                    c.into_owned()
                })
                .filter(|c| !c.is_empty())
                .collect();
            Cow::Owned(cleaned.join("\n\n"))
        } else {
            clean_text_cow(&item_content)
        };
        if !content.is_empty() && !ctx.list_stack.is_empty() {
            let indent = ctx.list_indent();
            let item = match ctx.list_stack.last_mut() {
                Some(ListType::Unordered) => {
                    format_unordered_item(content.as_ref(), indent, options.bullet_char)
                }
                Some(ListType::Ordered(num)) => {
                    let item = format_ordered_item(content.as_ref(), *num, indent);
                    *num += 1;
                    item
                }
                None => content.into_owned(),
            };

            if ctx.list_stack.len() == 1 && !ctx.output.is_empty() && !ctx.output.ends_with('\n') {
                ctx.output.push('\n');
            }
            ctx.emit(&item);
            ctx.output.push('\n');
            ctx.just_emitted_block = false;
        }
        item_content.clear(); // Clear so we don't emit again when </li> closes
        ctx.list_item_stack.push(item_content);
    }
}

/// Emit text to the appropriate context buffer.
fn emit_text_to_context<'a>(ctx: &mut Context<'a>, text: Cow<'a, str>, escape: bool) {
    let text = if escape && ctx.inline_stack.is_empty() && !ctx.in_link {
        maybe_escape_markdown(text)
    } else {
        text
    };
    let text_ref = text.as_ref();

    if text_ref.is_empty() {
        return;
    }

    if !ctx.inline_stack.is_empty() && should_append_text(ctx, text_ref) {
        open_inline_markers(ctx);
    }

    // After opening inline markers, trim leading whitespace from text
    // (whitespace should be outside the markers, not inside)
    let text_ref = if ctx.just_opened_inline {
        ctx.just_opened_inline = false;
        let trimmed = text_ref.trim_start();
        if trimmed.is_empty() {
            return;
        }
        trimmed
    } else {
        text_ref
    };

    // Add space after a just-closed inline marker if followed by a word character
    // (not punctuation like . , : ; ) ' etc.)
    if ctx.just_closed_inline {
        let first = text_ref.as_bytes()[0];
        if !first.is_ascii_whitespace() && !is_punctuation(first) {
            append_to_context(ctx, " ");
        }
        ctx.just_closed_inline = false;
    }

    append_to_context(ctx, text_ref);
}

fn should_append_text(ctx: &Context, text: &str) -> bool {
    if ctx.in_link || ctx.heading_level > 0 || ctx.in_paragraph {
        return true;
    }
    if ctx.list_item_stack.last().is_some() {
        return true;
    }
    !text.trim().is_empty()
}

fn append_to_context<'a>(ctx: &mut Context<'a>, text: &str) {
    // Flush any pending inline space before new content
    if ctx.pending_space {
        ctx.pending_space = false;
        // Only emit the space if the target buffer doesn't already end with whitespace
        if let Some(ch) = last_char_in_buffer(ctx)
            && !ch.is_whitespace()
        {
            append_to_context_inner(ctx, " ");
        }
    }
    append_to_context_inner(ctx, text);
}

fn append_to_context_inner<'a>(ctx: &mut Context<'a>, text: &str) {
    // Heading takes priority over link so that headings inside links
    // can be flattened to bold text by the heading close handler.
    if ctx.heading_level > 0 {
        ctx.heading_text.push_str(text);
        if ctx.in_link {
            ctx.link_text.push_str(text);
        }
        return;
    }
    if ctx.in_link {
        ctx.link_text.push_str(text);
        return;
    }
    if let Some(content) = ctx.list_item_stack.last_mut() {
        content.push_str(text);
        return;
    }
    if let Some(ref mut table) = ctx.table
        && table.in_cell
    {
        table.cell_content.push_str(text);
        return;
    }
    if ctx.in_paragraph {
        ctx.pending_text.push_str(text);
        return;
    }
    // Bare text outside any container
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        ctx.emit_inline(text);
    } else if !text.is_empty() && !ctx.output.is_empty() {
        // Whitespace-only text: preserve a single space if there's already output
        // (e.g., space inserted between text and a link)
        let last = ctx.output.as_bytes().last().copied().unwrap_or(b'\n');
        if last != b' ' && last != b'\n' {
            ctx.emit_inline(" ");
        }
    }
}

fn open_inline_markers<'a>(ctx: &mut Context<'a>) {
    let len = ctx.inline_stack.len();
    for idx in 0..len {
        // Skip nested entries — they don't emit markers
        if ctx.inline_stack[idx].nested {
            ctx.inline_stack[idx].opened = true;
            continue;
        }
        let marker = if !ctx.inline_stack[idx].opened {
            ctx.inline_stack[idx].opened = true;
            Some(inline_marker(ctx.inline_stack[idx].kind))
        } else {
            None
        };

        if let Some(marker) = marker {
            // Add space before marker if preceded by non-whitespace text
            if needs_space_before_inline(ctx) {
                append_to_context(ctx, " ");
                ctx.just_closed_inline = false;
            }
            append_to_context(ctx, marker);
            ctx.just_opened_inline = true;
        }
    }
}

fn emit_inline_marker<'a>(ctx: &mut Context<'a>, kind: InlineFormat) {
    append_to_context(ctx, inline_marker(kind));
}

/// Close an inline formatting marker (strong, emphasis, strikethrough).
/// Trims trailing whitespace from content and sets just_closed_inline flag.
fn close_inline_format<'a>(ctx: &mut Context<'a>, kind: InlineFormat) {
    // Find the last entry matching this kind (search from end)
    let pos = ctx.inline_stack.iter().rposition(|s| s.kind == kind);
    if let Some(idx) = pos {
        let state = ctx.inline_stack.remove(idx);
        if state.opened && !state.nested {
            // Trim trailing whitespace from content before closing marker
            trim_trailing_whitespace_in_buffer(ctx);
            emit_inline_marker(ctx, kind);
            ctx.just_closed_inline = true;
        }
    }
}

fn inline_marker(kind: InlineFormat) -> &'static str {
    match kind {
        InlineFormat::Strong => "**",
        InlineFormat::Emphasis => "_",
        InlineFormat::Strikethrough => "~~",
    }
}

/// Escape leading text that could be interpreted as a markdown list marker.
/// e.g., "- text" → "\- text", "1. text" → "1\. text"
fn escape_leading_list_marker(text: &str) -> String {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return text.to_string();
    }
    // Check for unordered list markers: - * +
    if bytes.len() >= 2
        && (bytes[0] == b'-' || bytes[0] == b'*' || bytes[0] == b'+')
        && bytes[1] == b' '
    {
        let mut result = String::with_capacity(text.len() + 1);
        result.push('\\');
        result.push_str(text);
        return result;
    }
    // Check for ordered list markers: {digits}. or {digits})
    if bytes[0].is_ascii_digit() {
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            let mut result = String::with_capacity(text.len() + 1);
            result.push_str(&text[..i]);
            result.push('\\');
            result.push_str(&text[i..]);
            return result;
        }
    }
    text.to_string()
}

fn maybe_escape_markdown<'a>(text: Cow<'a, str>) -> Cow<'a, str> {
    match text {
        Cow::Borrowed(value) => escape_markdown_cow(value),
        Cow::Owned(value) => {
            let escaped = escape_markdown_cow(&value);
            match escaped {
                Cow::Borrowed(_) => Cow::Owned(value),
                Cow::Owned(escaped) => Cow::Owned(escaped),
            }
        }
    }
}

/// Finalize conversion (flush pending content).
fn finalize<'a>(ctx: &mut Context<'a>) {
    // Flush any pending paragraph
    if ctx.in_paragraph && !ctx.pending_text.is_empty() {
        let mut pending = std::mem::take(&mut ctx.pending_text);
        let text = clean_text_cow(&pending);
        if !text.is_empty() {
            let escaped = escape_leading_list_marker(text.as_ref());
            ctx.emit_block(&escaped);
        }
        pending.clear();
        ctx.pending_text = pending;
    }

    // Flush any pending heading
    if ctx.heading_level > 0 && !ctx.heading_text.is_empty() {
        let mut heading_text = std::mem::take(&mut ctx.heading_text);
        let text = clean_text_cow(&heading_text);
        if !text.is_empty() {
            let heading = format_heading(ctx.heading_level, text.as_ref());
            ctx.emit_block(&heading);
        }
        heading_text.clear();
        ctx.heading_text = heading_text;
    }

    // Flush any pending code block
    if ctx.in_code_block {
        let code = format_code_block(&ctx.code_content, ctx.code_language);
        ctx.emit_block(&code);
    }

    // Flush any pending list items
    while let Some(item_content) = ctx.list_item_stack.pop() {
        if !item_content.is_empty() {
            let content = clean_text_cow(&item_content);
            if !ctx.list_stack.is_empty() {
                let indent = ctx.list_indent();
                let item = match ctx.list_stack.last() {
                    Some(ListType::Unordered) => {
                        format_unordered_item(content.as_ref(), indent, '-')
                    }
                    Some(ListType::Ordered(num)) => {
                        format_ordered_item(content.as_ref(), *num, indent)
                    }
                    None => content.into_owned(),
                };
                ctx.emit(&item);
                ctx.output.push('\n');
            }
        }
    }

    // Flush any pending table
    if let Some(table) = ctx.table.take() {
        let formatted = table.formatter.format();
        if !formatted.is_empty() {
            ctx.emit_block(&formatted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert_default(html: &str) -> String {
        convert(html, &Options::default())
    }

    #[test]
    fn test_simple_paragraph() {
        assert_eq!(convert_default("<p>Hello world</p>"), "Hello world\n");
    }

    #[test]
    fn test_heading() {
        assert_eq!(convert_default("<h1>Title</h1>"), "# Title\n");
        assert_eq!(convert_default("<h2>Subtitle</h2>"), "## Subtitle\n");
    }

    #[test]
    fn test_emphasis() {
        assert_eq!(convert_default("<p><em>italic</em></p>"), "_italic_\n");
        assert_eq!(convert_default("<p><i>italic</i></p>"), "_italic_\n");
    }

    #[test]
    fn test_strong() {
        assert_eq!(
            convert_default("<p><strong>bold</strong></p>"),
            "**bold**\n"
        );
        assert_eq!(convert_default("<p><b>bold</b></p>"), "**bold**\n");
    }

    #[test]
    fn test_nested_formatting() {
        assert_eq!(
            convert_default("<p><strong><em>bold italic</em></strong></p>"),
            "**_bold italic_**\n"
        );
    }

    #[test]
    fn test_link() {
        assert_eq!(
            convert_default(r#"<a href="https://example.com">Example</a>"#),
            "[Example](https://example.com)\n"
        );
    }

    #[test]
    fn test_image() {
        assert_eq!(
            convert_default(r#"<img src="image.png" alt="An image">"#),
            "![An image](image.png)\n"
        );
    }

    #[test]
    fn test_unordered_list() {
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        assert_eq!(convert_default(html), "- Item 1\n- Item 2\n");
    }

    #[test]
    fn test_ordered_list() {
        let html = "<ol><li>First</li><li>Second</li></ol>";
        assert_eq!(convert_default(html), "1. First\n2. Second\n");
    }

    #[test]
    fn test_nested_list() {
        let html = "<ul><li>Item 1<ul><li>Nested</li></ul></li></ul>";
        assert_eq!(convert_default(html), "- Item 1\n  - Nested\n");
    }

    #[test]
    fn test_code_inline() {
        assert_eq!(
            convert_default("<p>Use <code>printf</code> function</p>"),
            "Use `printf` function\n"
        );
    }

    #[test]
    fn test_code_block() {
        let html = "<pre><code>let x = 1;\nlet y = 2;</code></pre>";
        assert_eq!(convert_default(html), "```\nlet x = 1;\nlet y = 2;\n```\n");
    }

    #[test]
    fn test_code_block_with_language() {
        let html = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#;
        assert_eq!(convert_default(html), "```rust\nfn main() {}\n```\n");
    }

    #[test]
    fn test_blockquote() {
        assert_eq!(
            convert_default("<blockquote><p>Quote</p></blockquote>"),
            "> Quote\n"
        );
    }

    #[test]
    fn test_hr() {
        assert_eq!(
            convert_default("<p>Before</p><hr><p>After</p>"),
            "Before\n\n* * *\n\nAfter\n"
        );
    }

    #[test]
    fn test_br() {
        assert_eq!(
            convert_default("<p>Line 1<br>Line 2</p>"),
            "Line 1\n\nLine 2\n"
        );
    }

    #[test]
    fn test_strikethrough() {
        assert_eq!(
            convert_default("<p><del>deleted</del></p>"),
            "~~deleted~~\n"
        );
    }

    #[test]
    fn test_entity_decoding() {
        assert_eq!(
            convert_default("<p>&lt;html&gt; &amp; more</p>"),
            "<html> & more\n"
        );
    }

    #[test]
    fn test_table() {
        let html = r#"
            <table>
                <thead>
                    <tr><th>Name</th><th>Age</th></tr>
                </thead>
                <tbody>
                    <tr><td>Alice</td><td>30</td></tr>
                </tbody>
            </table>
        "#;
        let result = convert_default(html);
        assert!(result.contains("| Name"));
        assert!(result.contains("| Alice"));
    }

    #[test]
    fn test_table_inline_formatting() {
        // Test inline formatting inside table cells
        let html = r#"<table><tr><td><strong>Bold</strong></td><td><a href="/url">Link</a></td><td><i>Italic</i></td></tr></table>"#;
        let result = convert_default(html);
        assert!(
            result.contains("**Bold**"),
            "Expected **Bold**, got: {}",
            result
        );
        assert!(
            result.contains("[Link](/url)"),
            "Expected [Link](/url), got: {}",
            result
        );
        assert!(
            result.contains("_Italic_"),
            "Expected _Italic_, got: {}",
            result
        );
    }

    #[test]
    fn test_heading_in_link() {
        // Heading inside a link should emit heading with link inside
        let html = r#"<a href="/page.html"><h4>Heading A</h4><h3>Heading B</h3></a>"#;
        let result = convert_default(html);
        assert!(
            result.contains("#### [Heading A](/page.html)"),
            "got: {}",
            result
        );
        assert!(result.contains("### Heading B"), "got: {}", result);
    }

    #[test]
    fn test_complex_document() {
        let html = r#"
            <h1>Title</h1>
            <p>This is a <strong>bold</strong> paragraph with a <a href="url">link</a>.</p>
            <ul>
                <li>Item 1</li>
                <li>Item 2</li>
            </ul>
            <blockquote>
                <p>A quote</p>
            </blockquote>
        "#;
        let result = convert_default(html);
        assert!(result.contains("# Title"));
        assert!(result.contains("**bold**"));
        assert!(result.contains("[link](url)"));
        assert!(result.contains("- Item 1"));
        assert!(result.contains("> A quote"));
    }

    #[test]
    fn test_link_spacing() {
        // Text adjacent to links should have spaces
        assert_eq!(
            convert_default(r#"<p>x<a href="/x">x</a>x<a href="/x">x</a></p>"#),
            "x [x](/x) x [x](/x)\n"
        );
        // Also in bare text context (no wrapping <p>)
        let result = convert_default(r#"x<a href="/x">x</a>x"#);
        assert_eq!(result, "x [x](/x) x\n", "bare text: {:?}", result);
    }

    #[test]
    fn test_anchor_without_href() {
        // <a> without href is a named anchor, should not produce a link
        assert_eq!(
            convert_default(r#"<p>before<a id="top"></a>after</p>"#),
            "beforeafter\n"
        );
    }

    #[test]
    fn test_inline_code_preserves_whitespace() {
        // Inline code should preserve internal newlines
        let html = "<p>CSS: <code>\nbody {\n    color: yellow;\n}\n</code></p>";
        let result = convert_default(html);
        // The backtick-wrapped code should contain newlines, not be collapsed
        assert!(
            result.contains("body {\n"),
            "Inline code should preserve newlines, got: {:?}",
            result
        );
    }

    #[test]
    fn test_inline_code_span_whitespace() {
        // Syntax highlighters wrap tokens in <span> elements inside <code>.
        // Whitespace between spans must be preserved.
        let html = r#"<code><span class="kwd">const</span><span class="pln"> </span><span class="typ">Pi</span><span class="pln"> </span><span class="pun">=</span><span class="pln"> </span><span class="lit">3.14</span></code>"#;
        let result = convert_default(html);
        assert!(
            result.contains("const Pi = 3.14"),
            "Whitespace between spans in inline code must be preserved, got: {:?}",
            result
        );
    }

    #[test]
    fn test_multi_paragraph_list_item() {
        // <li> with multiple <p> elements should preserve paragraph breaks
        let html = "<ul><li><p>First</p><p>Second</p></li></ul>";
        let result = convert_default(html);
        assert!(
            result.contains("First\n\n"),
            "Multi-paragraph list item should have paragraph break, got: {:?}",
            result
        );
        assert!(
            result.contains("Second"),
            "Second paragraph should be present, got: {:?}",
            result
        );
    }
}
