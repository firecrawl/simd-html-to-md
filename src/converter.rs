//! Main HTML to Markdown conversion logic with state machine.

use crate::emitter::{
    clean_text_cow, escape_markdown_cow, format_br, format_code,
    format_code_block, format_heading, format_hr, format_image, format_link,
    format_ordered_item, format_unordered_item, Alignment,
    TableFormatter,
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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bullet_char: '-',
            code_block_fenced: true,
            preserve_line_breaks: false,
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
        }
    }

    /// Get current list item content mutably.
    fn current_list_item(&mut self) -> Option<&mut String> {
        self.list_item_stack.last_mut()
    }

    /// Ensure proper spacing before a block element.
    fn ensure_block_spacing(&mut self) {
        if !self.output.is_empty() && !self.just_emitted_block {
            if !self.output.ends_with("\n\n") {
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
        self.output.push_str("\n\n");
        self.just_emitted_block = true;
    }

    /// Emit inline content.
    fn emit_inline(&mut self, text: &str) {
        if self.needs_newline {
            self.output.push('\n');
            self.needs_newline = false;
        }
        self.emit(text);
    }

    /// Get current list indent level.
    fn list_indent(&self) -> usize {
        if self.list_stack.is_empty() {
            0
        } else {
            (self.list_stack.len() - 1) * 2
        }
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
    match token {
        Token::StartTag(tag) => process_start_tag(ctx, &tag, options),
        Token::EndTag(name) => process_end_tag(ctx, name, options),
        Token::SelfClosingTag(tag) => process_self_closing_tag(ctx, &tag),
        Token::Text(text) => process_text(ctx, text),
        Token::Comment(_) => {} // Ignore comments
        Token::Doctype(_) => {} // Ignore doctype
    }
}

/// Process a start tag.
fn process_start_tag<'a>(ctx: &mut Context<'a>, tag: &Tag<'a>, options: &Options) {
    match tag_kind(tag.name) {
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
            ctx.in_paragraph = true;
            ctx.pending_text.clear();
        }

        // Emphasis
        TagKind::Strong => {
            ctx.inline_stack.push(InlineState {
                kind: InlineFormat::Strong,
                opened: false,
            });
        }
        TagKind::Emphasis => {
            ctx.inline_stack.push(InlineState {
                kind: InlineFormat::Emphasis,
                opened: false,
            });
        }
        TagKind::Strikethrough => {
            ctx.inline_stack.push(InlineState {
                kind: InlineFormat::Strikethrough,
                opened: false,
            });
        }

        // Code
        TagKind::Code => {
            if ctx.in_pre {
                // Part of a code block - extract language from class
                if let Some(class) = tag.get_attr("class") {
                    // Look for language-* or lang-* class
                    for part in class.split_whitespace() {
                        if let Some(lang) = part.strip_prefix("language-") {
                            ctx.code_language = Some(lang);
                            break;
                        } else if let Some(lang) = part.strip_prefix("lang-") {
                            ctx.code_language = Some(lang);
                            break;
                        }
                    }
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
            ctx.code_language = None;
        }

        // Links
        TagKind::A => {
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
                cell_content: String::new(),
                current_row: Vec::new(),
                formatter: TableFormatter::new(),
            });
        }
        TagKind::Thead => {
            if let Some(ref mut table) = ctx.table {
                table.in_header = true;
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
            }
        }
        TagKind::Th | TagKind::Td => {
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

        // Div/span - just containers, process content
        TagKind::Div
        | TagKind::Span
        | TagKind::Section
        | TagKind::Article
        | TagKind::Header
        | TagKind::Footer
        | TagKind::Main
        | TagKind::Aside
        | TagKind::Nav => {
            // These are just containers, no special handling
        }

        _ => {
            // Unknown tag, ignore
        }
    }
}

/// Process an end tag.
fn process_end_tag<'a>(ctx: &mut Context<'a>, name: &str, options: &Options) {
    match tag_kind(name) {
        // Headings
        TagKind::H1 | TagKind::H2 | TagKind::H3 | TagKind::H4 | TagKind::H5 | TagKind::H6 => {
            if ctx.heading_level > 0 {
                let mut heading_text = std::mem::take(&mut ctx.heading_text);
                let text = clean_text_cow(&heading_text);
                if !text.is_empty() {
                    let heading = format_heading(ctx.heading_level, text.as_ref());
                    ctx.emit_block(&heading);
                }
                heading_text.clear();
                ctx.heading_text = heading_text;
                ctx.heading_level = 0;
            }
        }

        // Paragraph
        TagKind::P => {
            if ctx.in_paragraph {
                let mut pending = std::mem::take(&mut ctx.pending_text);
                let text = clean_text_cow(&pending);
                if !text.is_empty() {
                    ctx.emit_block(text.as_ref());
                }
                pending.clear();
                ctx.pending_text = pending;
                ctx.in_paragraph = false;
            }
        }

        // Emphasis
        TagKind::Strong => {
            if let Some(state) = ctx.inline_stack.last() {
                if state.kind == InlineFormat::Strong {
                    let state = ctx.inline_stack.pop().unwrap();
                    if state.opened {
                        emit_inline_marker(ctx, InlineFormat::Strong);
                    }
                }
            }
        }
        TagKind::Emphasis => {
            if let Some(state) = ctx.inline_stack.last() {
                if state.kind == InlineFormat::Emphasis {
                    let state = ctx.inline_stack.pop().unwrap();
                    if state.opened {
                        emit_inline_marker(ctx, InlineFormat::Emphasis);
                    }
                }
            }
        }
        TagKind::Strikethrough => {
            if let Some(state) = ctx.inline_stack.last() {
                if state.kind == InlineFormat::Strikethrough {
                    let state = ctx.inline_stack.pop().unwrap();
                    if state.opened {
                        emit_inline_marker(ctx, InlineFormat::Strikethrough);
                    }
                }
            }
        }

        // Code
        TagKind::Code => {
            if !ctx.in_pre {
                if ctx.in_code {
                    let code = format_code(&ctx.inline_code_content);
                    emit_text_to_context(ctx, Cow::Owned(code), false);
                    ctx.inline_code_content.clear();
                }
                ctx.in_code = false;
            }
        }

        // Preformatted / code blocks
        TagKind::Pre => {
            if ctx.in_code_block {
                let code = format_code_block(&ctx.code_content, ctx.code_language);
                ctx.emit_block(&code);
                ctx.in_code_block = false;
                ctx.in_pre = false;
                ctx.code_content.clear();
                ctx.code_language = None;
            }
        }

        // Links
        TagKind::A => {
            if ctx.in_link {
                let url = ctx.link_url.take().unwrap_or("");
                let title = ctx.link_title.take();
                let link = {
                    let text = clean_text_cow(&ctx.link_text);
                    let link_text = if text.is_empty() { url } else { text.as_ref() };
                    format_link(link_text, url, title)
                };
                ctx.in_link = false;
                ctx.link_text.clear();
                // Emit the link to the appropriate context
                emit_text_to_context(ctx, Cow::Owned(link), false);
            }
        }

        // Lists
        TagKind::Ul | TagKind::Ol => {
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
                let content = clean_text_cow(&item_content);
                // Only emit if there's content (might be empty if already flushed for nested list)
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

                    // For first list item, ensure proper spacing from previous content
                    if ctx.list_stack.len() == 1 && !ctx.output.is_empty() && !ctx.output.ends_with('\n') {
                        ctx.output.push('\n');
                    }
                    ctx.emit(&item);
                    ctx.output.push('\n');
                    ctx.just_emitted_block = false;
                }
            }
        }

        // Blockquote
        TagKind::Blockquote => {
            if ctx.blockquote_depth > 0 {
                ctx.blockquote_depth -= 1;
            }
        }

        // Table
        TagKind::Table => {
            if let Some(table) = ctx.table.take() {
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
            if let Some(ref mut table) = ctx.table {
                if table.in_row {
                    if table.in_header {
                        table.formatter.set_headers(table.current_row.clone());
                    } else {
                        table.formatter.add_row(table.current_row.clone());
                    }
                    table.current_row.clear();
                    table.in_row = false;
                }
            }
        }
        TagKind::Th | TagKind::Td => {
            if let Some(ref mut table) = ctx.table {
                if table.in_cell {
                    table
                        .current_row
                        .push(clean_text_cow(&table.cell_content).into_owned());
                    table.cell_content.clear();
                    table.in_cell = false;
                }
            }
        }

        _ => {}
    }
}

/// Process a self-closing tag.
fn process_self_closing_tag<'a>(ctx: &mut Context<'a>, tag: &Tag<'a>) {
    match tag_kind(tag.name) {
        // Line break
        TagKind::Br => {
            if ctx.in_code_block {
                ctx.code_content.push('\n');
            } else if let Some(content) = ctx.current_list_item() {
                content.push_str(format_br());
            } else if ctx.in_paragraph {
                ctx.pending_text.push_str(format_br());
            } else if ctx.heading_level > 0 {
                ctx.heading_text.push(' ');
            } else {
                ctx.emit_inline(format_br());
            }
        }

        // Horizontal rule
        TagKind::Hr => {
            ctx.emit_block(format_hr());
        }

        // Image
        TagKind::Img => {
            let src = tag.get_attr("src").unwrap_or("");
            let alt = tag.get_attr("alt").unwrap_or("");
            let title = tag.get_attr("title");

            let image = format_image(alt, src, title);
            emit_text_to_context(ctx, Cow::Owned(image), false);
        }

        // Input (for checkboxes in task lists)
        TagKind::Input => {
            if tag.get_attr("type") == Some("checkbox") {
                let checked = tag
                    .attributes
                    .iter()
                    .any(|a| a.name.eq_ignore_ascii_case("checked"));
                let checkbox = if checked { "[x] " } else { "[ ] " };
                emit_text_to_context(ctx, Cow::Borrowed(checkbox), false);
            }
        }

        _ => {}
    }
}

/// Process text content.
fn process_text<'a>(ctx: &mut Context<'a>, text: &'a str) {
    if !ctx.in_code_block
        && !ctx.in_paragraph
        && !ctx.in_link
        && ctx.heading_level == 0
        && ctx.list_item_stack.is_empty()
        && ctx.table.as_ref().map_or(true, |table| !table.in_cell)
        && is_ascii_whitespace_only(text)
    {
        // Ignore whitespace-only text between block elements.
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

    // Handle table cells
    if let Some(ref mut table) = ctx.table {
        if table.in_cell {
            if simd::find_amp(text.as_bytes()).is_none() {
                table.cell_content.push_str(text);
            } else {
                let decoded = decode_entities_cow(text);
                table.cell_content.push_str(decoded.as_ref());
            }
            return;
        }
    }

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
    let special = b"\\`*_{}[]()#!|<>&";

    if bytes.len() >= ESCAPE_SIMD_THRESHOLD {
        simd::find_any_index(bytes, special).is_some()
    } else {
        bytes.iter().any(|&b| {
            matches!(
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

// Inline formatting is streamed by emitting markers on tag open/close.

/// Flush the current list item content (used before starting a nested list).
fn flush_current_list_item<'a>(ctx: &mut Context<'a>, options: &Options) {
    if let Some(mut item_content) = ctx.list_item_stack.pop() {
        let content = clean_text_cow(&item_content);
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
    let text = text.as_ref();

    if !text.is_empty() && !ctx.inline_stack.is_empty() && should_append_text(ctx, text) {
        open_inline_markers(ctx);
    }

    append_to_context(ctx, text);
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
    if ctx.in_link {
        ctx.link_text.push_str(text);
    } else if ctx.heading_level > 0 {
        ctx.heading_text.push_str(text);
    } else if let Some(content) = ctx.current_list_item() {
        content.push_str(text);
    } else if ctx.in_paragraph {
        ctx.pending_text.push_str(text);
    } else {
        // Bare text outside any container
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            ctx.emit_inline(text);
        }
    }
}

fn open_inline_markers<'a>(ctx: &mut Context<'a>) {
    let len = ctx.inline_stack.len();
    for idx in 0..len {
        let marker = if !ctx.inline_stack[idx].opened {
            ctx.inline_stack[idx].opened = true;
            Some(inline_marker(ctx.inline_stack[idx].kind))
        } else {
            None
        };

        if let Some(marker) = marker {
            append_to_context(ctx, marker);
        }
    }
}

fn emit_inline_marker<'a>(ctx: &mut Context<'a>, kind: InlineFormat) {
    append_to_context(ctx, inline_marker(kind));
}

fn inline_marker(kind: InlineFormat) -> &'static str {
    match kind {
        InlineFormat::Strong => "**",
        InlineFormat::Emphasis => "*",
        InlineFormat::Strikethrough => "~~",
    }
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
            ctx.emit_block(text.as_ref());
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
        assert_eq!(convert_default("<p><em>italic</em></p>"), "*italic*\n");
        assert_eq!(convert_default("<p><i>italic</i></p>"), "*italic*\n");
    }

    #[test]
    fn test_strong() {
        assert_eq!(convert_default("<p><strong>bold</strong></p>"), "**bold**\n");
        assert_eq!(convert_default("<p><b>bold</b></p>"), "**bold**\n");
    }

    #[test]
    fn test_nested_formatting() {
        assert_eq!(
            convert_default("<p><strong><em>bold italic</em></strong></p>"),
            "***bold italic***\n"
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
        assert_eq!(
            convert_default(html),
            "```\nlet x = 1;\nlet y = 2;\n```\n"
        );
    }

    #[test]
    fn test_code_block_with_language() {
        let html = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#;
        assert_eq!(
            convert_default(html),
            "```rust\nfn main() {}\n```\n"
        );
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
        assert_eq!(convert_default("<p>Before</p><hr><p>After</p>"), "Before\n\n---\n\nAfter\n");
    }

    #[test]
    fn test_br() {
        assert_eq!(
            convert_default("<p>Line 1<br>Line 2</p>"),
            "Line 1  \nLine 2\n"
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
            "\\<html\\> \\& more\n"
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
}
