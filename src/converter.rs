//! Main HTML to Markdown conversion logic with state machine.

use crate::emitter::{
    clean_text_cow, escape_markdown_cow, format_br, format_code,
    format_code_block, format_emphasis, format_heading, format_hr, format_image, format_link,
    format_ordered_item, format_strikethrough, format_strong, format_unordered_item, Alignment,
    TableFormatter,
};
use crate::entities::decode_entities_cow;
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
struct Context {
    /// Stack of list types (for nesting).
    list_stack: Vec<ListType>,
    /// Blockquote depth.
    blockquote_depth: usize,
    /// Whether we're in a code block.
    in_code_block: bool,
    /// Whether we're in inline code.
    in_code: bool,
    /// Whether we're in a preformatted section.
    in_pre: bool,
    /// Code block language (from class attribute).
    code_language: Option<String>,
    /// Accumulated code block content.
    code_content: String,
    /// Stack of inline formatting.
    inline_stack: Vec<InlineFormat>,
    /// Link URL being built.
    link_url: Option<String>,
    /// Link title being built.
    link_title: Option<String>,
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
    Code,
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

impl Context {
    fn with_capacity(output_capacity: usize) -> Self {
        Self {
            list_stack: Vec::new(),
            blockquote_depth: 0,
            in_code_block: false,
            in_code: false,
            in_pre: false,
            code_language: None,
            code_content: String::new(),
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
fn process_token(ctx: &mut Context, token: Token<'_>, options: &Options) {
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
fn process_start_tag(ctx: &mut Context, tag: &Tag<'_>, options: &Options) {
    match tag.name {
        // Headings
        name if name.eq_ignore_ascii_case("h1") => {
            ctx.heading_level = 1;
            ctx.heading_text.clear();
        }
        name if name.eq_ignore_ascii_case("h2") => {
            ctx.heading_level = 2;
            ctx.heading_text.clear();
        }
        name if name.eq_ignore_ascii_case("h3") => {
            ctx.heading_level = 3;
            ctx.heading_text.clear();
        }
        name if name.eq_ignore_ascii_case("h4") => {
            ctx.heading_level = 4;
            ctx.heading_text.clear();
        }
        name if name.eq_ignore_ascii_case("h5") => {
            ctx.heading_level = 5;
            ctx.heading_text.clear();
        }
        name if name.eq_ignore_ascii_case("h6") => {
            ctx.heading_level = 6;
            ctx.heading_text.clear();
        }

        // Paragraph
        name if name.eq_ignore_ascii_case("p") => {
            ctx.in_paragraph = true;
            ctx.pending_text.clear();
        }

        // Emphasis
        name if name.eq_ignore_ascii_case("strong") || name.eq_ignore_ascii_case("b") => {
            ctx.inline_stack.push(InlineFormat::Strong);
        }
        name if name.eq_ignore_ascii_case("em") || name.eq_ignore_ascii_case("i") => {
            ctx.inline_stack.push(InlineFormat::Emphasis);
        }
        name
            if name.eq_ignore_ascii_case("del")
                || name.eq_ignore_ascii_case("s")
                || name.eq_ignore_ascii_case("strike") =>
        {
            ctx.inline_stack.push(InlineFormat::Strikethrough);
        }

        // Code
        name if name.eq_ignore_ascii_case("code") => {
            if ctx.in_pre {
                // Part of a code block - extract language from class
                if let Some(class) = tag.get_attr("class") {
                    // Look for language-* or lang-* class
                    for part in class.split_whitespace() {
                        if let Some(lang) = part.strip_prefix("language-") {
                            ctx.code_language = Some(lang.to_string());
                            break;
                        } else if let Some(lang) = part.strip_prefix("lang-") {
                            ctx.code_language = Some(lang.to_string());
                            break;
                        }
                    }
                }
            } else {
                ctx.in_code = true;
                ctx.inline_stack.push(InlineFormat::Code);
            }
        }

        // Preformatted / code blocks
        name if name.eq_ignore_ascii_case("pre") => {
            ctx.in_pre = true;
            ctx.in_code_block = true;
            ctx.code_content.clear();
            ctx.code_language = None;
        }

        // Links
        name if name.eq_ignore_ascii_case("a") => {
            ctx.in_link = true;
            ctx.link_url = tag.get_attr("href").map(|s| s.to_string());
            ctx.link_title = tag.get_attr("title").map(|s| s.to_string());
            ctx.link_text.clear();
        }

        // Lists
        name if name.eq_ignore_ascii_case("ul") => {
            // If we're in a list item, emit the current content before starting nested list
            if !ctx.list_item_stack.is_empty() {
                flush_current_list_item(ctx, options);
            }
            ctx.list_stack.push(ListType::Unordered);
        }
        name if name.eq_ignore_ascii_case("ol") => {
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
        name if name.eq_ignore_ascii_case("li") => {
            ctx.list_item_stack.push(String::new());
        }

        // Blockquote
        name if name.eq_ignore_ascii_case("blockquote") => {
            ctx.blockquote_depth += 1;
        }

        // Table
        name if name.eq_ignore_ascii_case("table") => {
            ctx.table = Some(TableState {
                in_header: false,
                in_row: false,
                in_cell: false,
                cell_content: String::new(),
                current_row: Vec::new(),
                formatter: TableFormatter::new(),
            });
        }
        name if name.eq_ignore_ascii_case("thead") => {
            if let Some(ref mut table) = ctx.table {
                table.in_header = true;
            }
        }
        name if name.eq_ignore_ascii_case("tbody") || name.eq_ignore_ascii_case("tfoot") => {
            if let Some(ref mut table) = ctx.table {
                table.in_header = false;
            }
        }
        name if name.eq_ignore_ascii_case("tr") => {
            if let Some(ref mut table) = ctx.table {
                table.in_row = true;
                table.current_row.clear();
            }
        }
        name if name.eq_ignore_ascii_case("th") || name.eq_ignore_ascii_case("td") => {
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
        name
            if name.eq_ignore_ascii_case("div")
                || name.eq_ignore_ascii_case("span")
                || name.eq_ignore_ascii_case("section")
                || name.eq_ignore_ascii_case("article")
                || name.eq_ignore_ascii_case("header")
                || name.eq_ignore_ascii_case("footer")
                || name.eq_ignore_ascii_case("main")
                || name.eq_ignore_ascii_case("aside")
                || name.eq_ignore_ascii_case("nav") =>
        {
            // These are just containers, no special handling
        }

        _ => {
            // Unknown tag, ignore
        }
    }
}

/// Process an end tag.
fn process_end_tag(ctx: &mut Context, name: &str, options: &Options) {
    match name {
        // Headings
        name
            if name.eq_ignore_ascii_case("h1")
                || name.eq_ignore_ascii_case("h2")
                || name.eq_ignore_ascii_case("h3")
                || name.eq_ignore_ascii_case("h4")
                || name.eq_ignore_ascii_case("h5")
                || name.eq_ignore_ascii_case("h6") =>
        {
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
        name if name.eq_ignore_ascii_case("p") => {
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
        name if name.eq_ignore_ascii_case("strong") || name.eq_ignore_ascii_case("b") => {
            if let Some(InlineFormat::Strong) = ctx.inline_stack.last() {
                ctx.inline_stack.pop();
            }
        }
        name if name.eq_ignore_ascii_case("em") || name.eq_ignore_ascii_case("i") => {
            if let Some(InlineFormat::Emphasis) = ctx.inline_stack.last() {
                ctx.inline_stack.pop();
            }
        }
        name
            if name.eq_ignore_ascii_case("del")
                || name.eq_ignore_ascii_case("s")
                || name.eq_ignore_ascii_case("strike") =>
        {
            if let Some(InlineFormat::Strikethrough) = ctx.inline_stack.last() {
                ctx.inline_stack.pop();
            }
        }

        // Code
        name if name.eq_ignore_ascii_case("code") => {
            if !ctx.in_pre {
                if let Some(InlineFormat::Code) = ctx.inline_stack.last() {
                    ctx.inline_stack.pop();
                }
                ctx.in_code = false;
            }
        }

        // Preformatted / code blocks
        name if name.eq_ignore_ascii_case("pre") => {
            if ctx.in_code_block {
                let code = format_code_block(&ctx.code_content, ctx.code_language.as_deref());
                ctx.emit_block(&code);
                ctx.in_code_block = false;
                ctx.in_pre = false;
                ctx.code_content.clear();
                ctx.code_language = None;
            }
        }

        // Links
        name if name.eq_ignore_ascii_case("a") => {
            if ctx.in_link {
                let url = ctx.link_url.take().unwrap_or_default();
                let title = ctx.link_title.take();
                let link = {
                    let text = clean_text_cow(&ctx.link_text);
                    let link_text = if text.is_empty() {
                        url.as_str()
                    } else {
                        text.as_ref()
                    };
                    format_link(link_text, &url, title.as_deref())
                };
                ctx.in_link = false;
                ctx.link_text.clear();
                // Emit the link to the appropriate context
                emit_text_to_context(ctx, Cow::Owned(link), false);
            }
        }

        // Lists
        name if name.eq_ignore_ascii_case("ul") || name.eq_ignore_ascii_case("ol") => {
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
        name if name.eq_ignore_ascii_case("li") => {
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
        name if name.eq_ignore_ascii_case("blockquote") => {
            if ctx.blockquote_depth > 0 {
                ctx.blockquote_depth -= 1;
            }
        }

        // Table
        name if name.eq_ignore_ascii_case("table") => {
            if let Some(table) = ctx.table.take() {
                let formatted = table.formatter.format();
                if !formatted.is_empty() {
                    ctx.emit_block(&formatted);
                }
            }
        }
        name if name.eq_ignore_ascii_case("thead") => {
            if let Some(ref mut table) = ctx.table {
                table.in_header = false;
            }
        }
        name if name.eq_ignore_ascii_case("tr") => {
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
        name if name.eq_ignore_ascii_case("th") || name.eq_ignore_ascii_case("td") => {
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
fn process_self_closing_tag(ctx: &mut Context, tag: &Tag<'_>) {
    match tag.name {
        // Line break
        name if name.eq_ignore_ascii_case("br") => {
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
        name if name.eq_ignore_ascii_case("hr") => {
            ctx.emit_block(format_hr());
        }

        // Image
        name if name.eq_ignore_ascii_case("img") => {
            let src = tag.get_attr("src").unwrap_or("");
            let alt = tag.get_attr("alt").unwrap_or("");
            let title = tag.get_attr("title");

            let image = format_image(alt, src, title);
            emit_text_to_context(ctx, Cow::Owned(image), false);
        }

        // Input (for checkboxes in task lists)
        name if name.eq_ignore_ascii_case("input") => {
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
fn process_text(ctx: &mut Context, text: &str) {
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
    let decoded = decode_entities_cow(text);

    // Handle code blocks specially - preserve all whitespace
    if ctx.in_code_block {
        ctx.code_content.push_str(decoded.as_ref());
        return;
    }

    // Handle table cells
    if let Some(ref mut table) = ctx.table {
        if table.in_cell {
            table.cell_content.push_str(decoded.as_ref());
            return;
        }
    }

    // Apply inline formatting
    let formatted = apply_inline_formatting(ctx, decoded);

    emit_text_to_context(ctx, formatted, true);
}

fn is_ascii_whitespace_only(text: &str) -> bool {
    text.as_bytes().iter().all(|b| b.is_ascii_whitespace())
}

/// Apply inline formatting to text.
fn apply_inline_formatting<'a>(ctx: &Context, text: Cow<'a, str>) -> Cow<'a, str> {
    if ctx.inline_stack.is_empty() {
        return text;
    }

    let mut result = text.into_owned();
    // Apply formatting in reverse order (innermost first)
    for format in ctx.inline_stack.iter().rev() {
        result = match format {
            InlineFormat::Strong => format_strong(&result),
            InlineFormat::Emphasis => format_emphasis(&result),
            InlineFormat::Strikethrough => format_strikethrough(&result),
            InlineFormat::Code => format_code(&result),
        };
    }

    Cow::Owned(result)
}

/// Flush the current list item content (used before starting a nested list).
fn flush_current_list_item(ctx: &mut Context, options: &Options) {
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
fn emit_text_to_context(ctx: &mut Context, text: Cow<'_, str>, escape: bool) {
    let text = if escape && ctx.inline_stack.is_empty() && !ctx.in_link {
        maybe_escape_markdown(text)
    } else {
        text
    };
    let text = text.as_ref();

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
fn finalize(ctx: &mut Context) {
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
        let code = format_code_block(&ctx.code_content, ctx.code_language.as_deref());
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
