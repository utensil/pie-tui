//! Markdown component backed by an owned mdast tree and exact source ranges.
//!
//! Parsing is delegated to `markdown`'s GFM token model. Terminal rendering,
//! callback ordering, cache behavior, and the small marked-compatibility layer
//! are clean-room behavior specified by pinned black-box vectors.

use std::cell::OnceCell;
use std::collections::BTreeMap;

use markdown::mdast::{Definition, List, ListItem, Node};
use pie_core::latex::{RenderLatexOptions, render_latex};
use pie_core::text::{strip_terminal_sequences, visible_width};
use pie_core::wrap::{apply_background_to_line, wrap_text_with_ansi};

use crate::{Component, StyleFn};

/// Optional base styling applied to ordinary Markdown text.
#[derive(Default)]
pub struct DefaultTextStyle {
    pub color: Option<StyleFn>,
    pub bg_color: Option<StyleFn>,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
}

/// Syntax-highlighting callback for fenced code blocks.
pub type HighlightCodeFn = Box<dyn Fn(&str, Option<&str>) -> Vec<String> + Send>;

/// Theme callbacks for Markdown elements.
pub struct MarkdownTheme {
    pub heading: StyleFn,
    pub link: StyleFn,
    pub link_url: StyleFn,
    pub code: StyleFn,
    pub code_block: StyleFn,
    pub code_block_border: StyleFn,
    pub quote: StyleFn,
    pub quote_border: StyleFn,
    pub hr: StyleFn,
    pub list_bullet: StyleFn,
    pub bold: StyleFn,
    pub italic: StyleFn,
    pub strikethrough: StyleFn,
    pub underline: StyleFn,
    pub highlight_code: Option<HighlightCodeFn>,
    /// Prefix for each fenced code line. The reference default is two spaces.
    pub code_block_indent: Option<String>,
}

/// Source transform callback, passed the exact content width.
pub type MarkdownTransformFn = Box<dyn Fn(&str, usize) -> String + Send>;

/// Markdown rendering options.
pub struct MarkdownOptions {
    pub preserve_ordered_list_markers: bool,
    pub preserve_backslash_escapes: bool,
    pub transform: Option<MarkdownTransformFn>,
    pub render_latex: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            preserve_ordered_list_markers: false,
            preserve_backslash_escapes: false,
            transform: None,
            render_latex: true,
        }
    }
}

#[derive(Default)]
struct MarkdownCache {
    text: String,
    width: usize,
    lines: Option<Vec<String>>,
}

/// Cached, width-aware Markdown component.
pub struct Markdown {
    text: String,
    padding_x: usize,
    padding_y: usize,
    default_text_style: Option<DefaultTextStyle>,
    default_style_prefix: OnceCell<String>,
    theme: MarkdownTheme,
    options: MarkdownOptions,
    cache: MarkdownCache,
}

impl Markdown {
    pub fn new(
        text: impl Into<String>,
        padding_x: usize,
        padding_y: usize,
        theme: MarkdownTheme,
        default_text_style: Option<DefaultTextStyle>,
        options: MarkdownOptions,
    ) -> Self {
        Self {
            text: text.into(),
            padding_x,
            padding_y,
            default_text_style,
            default_style_prefix: OnceCell::new(),
            theme,
            options,
            cache: MarkdownCache::default(),
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cache = MarkdownCache::default();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn render_uncached(&mut self, width: usize) -> Vec<String> {
        let content_width = width.saturating_sub(self.padding_x * 2).max(1);
        let transformed = self.options.transform.as_ref().map_or_else(
            || self.text.clone(),
            |transform| transform(&self.text, content_width),
        );
        // pi-tui normalizes tabs after the transform and uses three spaces
        // regardless of the surrounding block kind.
        let source = transformed
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "   ");
        let parser_source = normalize_marked_ordered_interrupts(&source);
        let mut parse_options = markdown::ParseOptions::gfm();
        if self.options.render_latex {
            parse_options.constructs.math_flow = true;
            parse_options.constructs.math_text = true;
        }
        let rendered = markdown::to_mdast(&parser_source, &parse_options).map_or_else(
            |_| wrap_text_with_ansi(&self.apply_default_style(&source), content_width),
            |tree| self.render_document(&tree, &source, content_width),
        );
        if rendered.is_empty() {
            return Vec::new();
        }

        let mut finished = Vec::with_capacity(rendered.len());
        for line in rendered {
            let with_left_padding = format!("{}{line}", " ".repeat(self.padding_x));
            finished.push(self.finish_line(&with_left_padding, width));
        }
        // Compute content backgrounds before the reusable padding blank. The
        // reference invokes the background callback in exactly that order.
        let blank = self.finish_line("", width);
        let mut lines = Vec::with_capacity(finished.len() + self.padding_y * 2);
        for _ in 0..self.padding_y {
            lines.push(blank.clone());
        }
        lines.extend(finished);
        for _ in 0..self.padding_y {
            lines.push(blank.clone());
        }
        lines
    }

    fn finish_line(&self, line: &str, width: usize) -> String {
        if let Some(style) = self.default_text_style.as_ref()
            && let Some(background) = style.bg_color.as_ref()
        {
            return apply_background_to_line(line, width, background);
        }
        format!(
            "{line}{}",
            " ".repeat(width.saturating_sub(visible_width(line)))
        )
    }

    fn render_document(&self, root: &Node, source: &str, width: usize) -> Vec<String> {
        let Some(children) = root.children() else {
            return Vec::new();
        };
        if children.is_empty() {
            return Vec::new();
        }

        let definitions = collect_definitions(children);
        let mut output = Vec::new();
        if let Some(first) = children.first().and_then(Node::position)
            && boundary_has_blank(&source[..first.start.offset])
        {
            output.push(String::new());
        }

        for (index, child) in children.iter().enumerate() {
            if index > 0 {
                output.push(String::new());
            }
            output.extend(self.render_block(child, source, width, 0, &definitions));
        }

        if let Some(last) = children.last().and_then(Node::position)
            && boundary_has_blank(&source[last.end.offset..])
        {
            output.push(String::new());
        }
        output
    }

    fn render_block(
        &self,
        node: &Node,
        source: &str,
        width: usize,
        depth: usize,
        definitions: &BTreeMap<String, DefinitionInfo>,
    ) -> Vec<String> {
        match node {
            Node::Paragraph(paragraph) => {
                if let Some(math) = self.display_math_source(node, source) {
                    return self
                        .render_display_math(math, raw_node_source(node, source).unwrap_or(math));
                }
                let inline = self.render_inline_nodes(&paragraph.children, source, definitions);
                wrap_text_with_ansi(&inline, width)
            }
            Node::Heading(heading) => {
                let inline = self.render_inline_nodes(&heading.children, source, definitions);
                let emphasized = if heading.depth == 1 {
                    (self.theme.bold)(&(self.theme.underline)(&inline))
                } else {
                    (self.theme.bold)(&inline)
                };
                wrap_text_with_ansi(&(self.theme.heading)(&emphasized), width)
            }
            Node::Code(code) => {
                self.render_code_block(code.lang.as_deref().unwrap_or(""), &code.value, width)
            }
            Node::Blockquote(quote) => {
                let inner_width = width.saturating_sub(2).max(1);
                let mut output = Vec::new();
                for (index, child) in quote.children.iter().enumerate() {
                    if index > 0 {
                        output.extend(self.render_quote_payload("", inner_width));
                    }
                    let child_lines =
                        self.render_block(child, source, inner_width, depth, definitions);
                    if matches!(child, Node::Blockquote(_)) {
                        for line in child_lines {
                            output.extend(self.render_quote_payload(&line, inner_width));
                        }
                    } else {
                        output.extend(
                            self.render_quote_payload(&child_lines.join("\n"), inner_width),
                        );
                    }
                }
                output
            }
            Node::List(list) => self.render_list(list, source, width, depth, definitions),
            Node::Table(table) => {
                let rows = table
                    .children
                    .iter()
                    .filter_map(|row| match row {
                        Node::TableRow(row) => Some(
                            row.children
                                .iter()
                                .filter_map(|cell| match cell {
                                    Node::TableCell(cell) => Some(self.render_inline_nodes(
                                        &cell.children,
                                        source,
                                        definitions,
                                    )),
                                    _ => None,
                                })
                                .collect::<Vec<_>>(),
                        ),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                self.render_table(&rows, width)
            }
            Node::ThematicBreak(_) => vec![(self.theme.hr)(&"─".repeat(width))],
            Node::Math(math) => self.render_display_math(
                &math.value,
                raw_node_source(node, source).unwrap_or(&math.value),
            ),
            Node::Definition(_) => Vec::new(),
            _ => {
                if let Some(children) = node.children() {
                    let inline = self.render_inline_nodes(children, source, definitions);
                    wrap_text_with_ansi(&inline, width)
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn render_quote_payload(&self, content: &str, width: usize) -> Vec<String> {
        let styled = (self.theme.quote)(&(self.theme.italic)(content));
        wrap_text_with_ansi(&styled, width)
            .into_iter()
            .map(|line| format!("{}{}", (self.theme.quote_border)("│ "), line))
            .collect()
    }

    fn render_code_block(&self, language: &str, code: &str, width: usize) -> Vec<String> {
        let language = (!language.is_empty()).then_some(language);
        let opening = (self.theme.code_block_border)(&format!("```{}", language.unwrap_or("")));
        let lines = if let Some(highlight) = self.theme.highlight_code.as_ref() {
            highlight(code, language)
        } else {
            code.split('\n')
                .map(|line| (self.theme.code_block)(line))
                .collect()
        };
        let closing = (self.theme.code_block_border)("```");
        let indent = self.theme.code_block_indent.as_deref().unwrap_or("  ");
        let mut output = Vec::with_capacity(lines.len() + 2);
        output.push(opening);
        for line in lines {
            output.extend(wrap_text_with_ansi(&format!("{indent}{line}"), width));
        }
        output.push(closing);
        output
    }

    fn render_list(
        &self,
        list: &List,
        source: &str,
        width: usize,
        depth: usize,
        definitions: &BTreeMap<String, DefinitionInfo>,
    ) -> Vec<String> {
        let first_number = list
            .children
            .iter()
            .find_map(|node| original_list_marker(node, source).and_then(|marker| marker.number))
            .or(list.start)
            .unwrap_or(1);
        let mut output = Vec::new();

        for (item_index, child) in list.children.iter().enumerate() {
            let Node::ListItem(item) = child else {
                continue;
            };
            let original = original_list_marker(child, source);
            let marker = if list.ordered {
                let number = if self.options.preserve_ordered_list_markers {
                    original
                        .as_ref()
                        .and_then(|marker| marker.number)
                        .unwrap_or(first_number + item_index as u32)
                } else {
                    first_number + item_index as u32
                };
                format!("{number}.")
            } else {
                original
                    .as_ref()
                    .and_then(|marker| marker.bullet)
                    .unwrap_or('-')
                    .to_string()
            };
            self.render_list_item(
                item,
                &marker,
                source,
                width,
                depth,
                definitions,
                &mut output,
            );
            if list.spread && item_index + 1 < list.children.len() {
                output.push(String::new());
            }
        }
        output
    }

    #[allow(clippy::too_many_arguments)]
    fn render_list_item(
        &self,
        item: &ListItem,
        marker: &str,
        source: &str,
        width: usize,
        depth: usize,
        definitions: &BTreeMap<String, DefinitionInfo>,
        output: &mut Vec<String>,
    ) {
        let indent = " ".repeat(depth * 4);
        let marker = original_task_marker(item, source)
            .map_or_else(|| format!("{marker} "), |task| format!("{marker} {task} "));
        let styled_marker = (self.theme.list_bullet)(&marker);
        let prefix = format!("{indent}{styled_marker}");
        let continuation = " ".repeat(visible_width(&prefix));
        let content_width = width.saturating_sub(visible_width(&prefix)).max(1);
        let mut emitted_primary = false;

        for (child_index, child) in item.children.iter().enumerate() {
            match child {
                Node::Paragraph(paragraph) => {
                    if child_index > 0 && item.spread {
                        output.push(String::new());
                    }
                    let inline = self.render_inline_nodes(&paragraph.children, source, definitions);
                    let lines = wrap_text_with_ansi(&inline, content_width);
                    for (line_index, line) in lines.into_iter().enumerate() {
                        if !emitted_primary && line_index == 0 {
                            output.push(format!("{prefix}{line}"));
                        } else {
                            output.push(format!("{continuation}{line}"));
                        }
                    }
                    emitted_primary = true;
                }
                Node::List(nested) => {
                    output.extend(self.render_list(nested, source, width, depth + 1, definitions));
                }
                _ => {
                    let lines =
                        self.render_block(child, source, content_width, depth + 1, definitions);
                    for line in lines {
                        output.push(format!("{continuation}{line}"));
                    }
                    emitted_primary = true;
                }
            }
        }
        if !emitted_primary {
            output.push(prefix.trim_end().to_string());
        }
    }

    fn render_table(&self, rows: &[Vec<String>], width: usize) -> Vec<String> {
        let columns = rows.first().map(Vec::len).unwrap_or(0);
        if columns == 0 {
            return Vec::new();
        }
        let mut natural = vec![1usize; columns];
        let mut minimum = vec![1usize; columns];
        for row in rows {
            for (column, cell) in row.iter().take(columns).enumerate() {
                natural[column] = natural[column].max(visible_width(cell));
                minimum[column] = minimum[column].max(longest_word_width(cell));
            }
        }
        let cell_budget = width.saturating_sub(columns + 1 + columns * 2).max(columns);
        let column_widths = allocate_table_columns(&natural, &minimum, cell_budget);
        let mut output = vec![table_rule(&column_widths, '┌', '┬', '┐')];
        for (row_index, row) in rows.iter().enumerate() {
            let wrapped = (0..columns)
                .map(|column| {
                    wrap_text_with_ansi(
                        row.get(column).map(String::as_str).unwrap_or(""),
                        column_widths[column],
                    )
                })
                .collect::<Vec<_>>();
            let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
            for line_index in 0..height {
                let mut line = String::from("│");
                for (column, cell_lines) in wrapped.iter().enumerate() {
                    let cell = cell_lines.get(line_index).map(String::as_str).unwrap_or("");
                    line.push(' ');
                    line.push_str(cell);
                    line.push_str(
                        &" ".repeat(column_widths[column].saturating_sub(visible_width(cell))),
                    );
                    line.push_str(" │");
                }
                output.push(line);
            }
            if row_index + 1 < rows.len() {
                output.push(table_rule(&column_widths, '├', '┼', '┤'));
            }
        }
        output.push(table_rule(&column_widths, '└', '┴', '┘'));
        output
    }

    fn render_inline_nodes(
        &self,
        nodes: &[Node],
        source: &str,
        definitions: &BTreeMap<String, DefinitionInfo>,
    ) -> String {
        // The reference initializes the restoration prefix when it enters the
        // inline renderer, after transform/parse and before any inline span.
        // Pure display math never enters this path and therefore never pays
        // the synthetic NUL callback cost.
        let _ = self.default_style_prefix();
        let mut output = String::new();
        for node in nodes {
            match node {
                Node::Text(text) => {
                    let raw = raw_node_source(node, source).unwrap_or(&text.value);
                    let value = if raw.contains('\\') {
                        self.render_escaped_source(raw)
                    } else if has_character_reference(raw) {
                        raw.to_string()
                    } else {
                        text.value.clone()
                    };
                    output.push_str(&self.render_text_with_math(&value));
                }
                Node::Strong(strong) => {
                    let value = self.render_inline_nodes(&strong.children, source, definitions);
                    output.push_str(&(self.theme.bold)(&value));
                    output.push_str(self.default_style_prefix());
                }
                Node::Emphasis(emphasis) => {
                    let value = self.render_inline_nodes(&emphasis.children, source, definitions);
                    output.push_str(&(self.theme.italic)(&value));
                    output.push_str(self.default_style_prefix());
                }
                Node::Delete(delete) => {
                    let value = self.render_inline_nodes(&delete.children, source, definitions);
                    output.push_str(&(self.theme.strikethrough)(&value));
                    output.push_str(self.default_style_prefix());
                }
                Node::InlineCode(code) => {
                    output.push_str(&(self.theme.code)(&code.value));
                    output.push_str(self.default_style_prefix());
                }
                Node::Break(_) => output.push('\n'),
                Node::Link(link) => {
                    let label = self.render_inline_nodes(&link.children, source, definitions);
                    output.push_str(&(self.theme.link)(&(self.theme.underline)(&label)));
                    if !is_autolink(node, source) {
                        output.push_str(&(self.theme.link_url)(&format!(" ({})", link.url)));
                    }
                    output.push_str(self.default_style_prefix());
                }
                Node::LinkReference(link) => {
                    let label = self.render_inline_nodes(&link.children, source, definitions);
                    output.push_str(&(self.theme.link)(&(self.theme.underline)(&label)));
                    if let Some(definition) = definitions.get(&link.identifier) {
                        output.push_str(&(self.theme.link_url)(&format!(" ({})", definition.url)));
                    }
                    output.push_str(self.default_style_prefix());
                }
                Node::Image(image) => output.push_str(&self.apply_default_style(&image.alt)),
                Node::ImageReference(image) => {
                    output.push_str(&self.apply_default_style(&image.alt));
                }
                Node::InlineMath(math) => {
                    let raw = raw_node_source(node, source).unwrap_or(&math.value);
                    output.push_str(&self.render_inline_math(&math.value, raw));
                }
                Node::Html(html) => output.push_str(
                    &self.apply_default_style(raw_node_source(node, source).unwrap_or(&html.value)),
                ),
                _ => {
                    if let Some(children) = node.children() {
                        output.push_str(&self.render_inline_nodes(children, source, definitions));
                    }
                }
            }
        }
        output
    }

    fn render_escaped_source(&self, source: &str) -> String {
        let mut output = String::new();
        let mut chars = source.chars();
        while let Some(character) = chars.next() {
            if character == '\\'
                && let Some(escaped) = chars.next()
            {
                if matches!(escaped, '[' | ']') {
                    continue;
                }
                if is_markdown_escapable(escaped) {
                    if self.options.preserve_backslash_escapes {
                        output.push('\\');
                    }
                    output.push(escaped);
                } else {
                    output.push('\\');
                    output.push(escaped);
                }
            } else {
                output.push(character);
            }
        }
        output
    }

    fn render_text_with_math(&self, text: &str) -> String {
        if !self.options.render_latex || !text.contains('$') {
            return self.apply_default_style(text);
        }
        let mut output = String::new();
        let mut rest = text;
        while let Some(start) = rest.find('$') {
            let (plain, after_plain) = rest.split_at(start);
            output.push_str(&self.apply_default_style(plain));
            if let Some(after_display_marker) = after_plain.strip_prefix("$$") {
                output.push_str(&self.apply_default_style("$$"));
                rest = after_display_marker;
                continue;
            }
            let after_open = &after_plain[1..];
            let Some(end) = after_open.find('$') else {
                output.push_str(&self.apply_default_style(after_plain));
                return output;
            };
            output.push_str(&self.render_inline_math(&after_open[..end], &after_plain[..end + 2]));
            rest = &after_open[end + 1..];
        }
        output.push_str(&self.apply_default_style(rest));
        output
    }

    fn render_inline_math(&self, source: &str, raw: &str) -> String {
        let rendered =
            render_latex(source, RenderLatexOptions::default()).unwrap_or_else(|| raw.to_string());
        self.apply_math_style(&rendered)
    }

    fn display_math_source<'a>(&self, node: &Node, source: &'a str) -> Option<&'a str> {
        if !self.options.render_latex {
            return None;
        }
        let position = node.position()?;
        let raw = source
            .get(position.start.offset..position.end.offset)?
            .trim();
        let body = raw.strip_prefix("$$")?.strip_suffix("$$")?;
        Some(body.trim())
    }

    fn render_display_math(&self, source: &str, raw: &str) -> Vec<String> {
        render_latex(source, RenderLatexOptions { display: true })
            .map(|rendered| rendered.split('\n').map(str::to_string).collect())
            .unwrap_or_else(|| vec![raw.trim().to_string()])
            .into_iter()
            .map(|line| self.apply_math_style(&line))
            .collect()
    }

    fn apply_math_style(&self, text: &str) -> String {
        self.apply_default_style(text)
    }

    fn apply_default_style(&self, text: &str) -> String {
        apply_default_style_parts(&self.theme, self.default_text_style.as_ref(), text)
    }

    fn default_style_prefix(&self) -> &str {
        self.default_style_prefix
            .get_or_init(|| {
                let styled_nul =
                    apply_default_style_parts(&self.theme, self.default_text_style.as_ref(), "\0");
                styled_nul
                    .find('\0')
                    .map_or_else(String::new, |position| styled_nul[..position].to_string())
            })
            .as_str()
    }
}

fn is_markdown_escapable(character: char) -> bool {
    character.is_ascii_punctuation()
}

fn apply_default_style_parts(
    theme: &MarkdownTheme,
    style: Option<&DefaultTextStyle>,
    text: &str,
) -> String {
    let Some(style) = style else {
        return text.to_string();
    };
    let mut output = style
        .color
        .as_ref()
        .map_or_else(|| text.to_string(), |color| color(text));
    if style.bold {
        output = (theme.bold)(&output);
    }
    if style.italic {
        output = (theme.italic)(&output);
    }
    if style.strikethrough {
        output = (theme.strikethrough)(&output);
    }
    if style.underline {
        output = (theme.underline)(&output);
    }
    output
}

impl Component for Markdown {
    fn invalidate(&mut self) {
        self.cache = MarkdownCache::default();
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        if self.cache.lines.is_some() && self.cache.text == self.text && self.cache.width == width {
            return self.cache.lines.clone().unwrap();
        }
        let lines = self.render_uncached(width);
        self.cache = MarkdownCache {
            text: self.text.clone(),
            width,
            lines: Some(lines.clone()),
        };
        lines
    }
}

#[derive(Clone)]
struct DefinitionInfo {
    url: String,
}

fn collect_definitions(children: &[Node]) -> BTreeMap<String, DefinitionInfo> {
    children
        .iter()
        .filter_map(|node| match node {
            Node::Definition(Definition {
                identifier, url, ..
            }) => Some((identifier.clone(), DefinitionInfo { url: url.clone() })),
            _ => None,
        })
        .collect()
}

fn boundary_has_blank(source: &str) -> bool {
    source.bytes().filter(|byte| *byte == b'\n').count() >= 2
}

fn is_autolink(node: &Node, source: &str) -> bool {
    node.position()
        .and_then(|position| source.get(position.start.offset..position.end.offset))
        .is_some_and(|slice| {
            let trimmed = slice.trim();
            trimmed.starts_with('<') && trimmed.ends_with('>')
        })
}

fn raw_node_source<'a>(node: &Node, source: &'a str) -> Option<&'a str> {
    let position = node.position()?;
    source.get(position.start.offset..position.end.offset)
}

fn has_character_reference(source: &str) -> bool {
    source.match_indices('&').any(|(start, _)| {
        let Some(tail) = source.get(start + 1..) else {
            return false;
        };
        let Some(end) = tail.find(';') else {
            return false;
        };
        let body = &tail[..end];
        if let Some(numeric) = body.strip_prefix('#') {
            if let Some(hex) = numeric
                .strip_prefix('x')
                .or_else(|| numeric.strip_prefix('X'))
            {
                !hex.is_empty() && hex.chars().all(|character| character.is_ascii_hexdigit())
            } else {
                !numeric.is_empty() && numeric.chars().all(|character| character.is_ascii_digit())
            }
        } else {
            !body.is_empty()
                && body
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        }
    })
}

#[derive(Debug)]
struct OriginalListMarker {
    number: Option<u32>,
    bullet: Option<char>,
}

fn original_list_marker(node: &Node, source: &str) -> Option<OriginalListMarker> {
    let position = node.position()?;
    let tail = source.get(position.start.offset..)?;
    let line = tail.lines().next().unwrap_or(tail);
    let body = line.trim_start_matches(' ');
    if let Some(bullet) = body.chars().next()
        && matches!(bullet, '-' | '+' | '*')
        && body[bullet.len_utf8()..].starts_with(char::is_whitespace)
    {
        return Some(OriginalListMarker {
            number: None,
            bullet: Some(bullet),
        });
    }
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let suffix = body.as_bytes().get(digits).copied()?;
    if !matches!(suffix, b'.' | b')') {
        return None;
    }
    Some(OriginalListMarker {
        number: body[..digits].parse().ok(),
        bullet: None,
    })
}

fn original_task_marker<'a>(item: &ListItem, source: &'a str) -> Option<&'a str> {
    item.checked?;
    let position = item.position.as_ref()?;
    let tail = source.get(position.start.offset..)?;
    let line = tail.lines().next().unwrap_or(tail);
    let body = line.trim_start_matches(' ');
    let marker_end = if body
        .as_bytes()
        .first()
        .is_some_and(|byte| matches!(byte, b'-' | b'+' | b'*'))
    {
        1
    } else {
        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || !matches!(body.as_bytes().get(digits), Some(b'.' | b')')) {
            return None;
        }
        digits + 1
    };
    let after_marker = body.get(marker_end..)?;
    if !after_marker.starts_with(char::is_whitespace) {
        return None;
    }
    let candidate = after_marker.trim_start_matches(char::is_whitespace);
    let bytes = candidate.as_bytes();
    if bytes.len() < 3
        || bytes[0] != b'['
        || !matches!(bytes[1], b' ' | b'x' | b'X')
        || bytes[2] != b']'
    {
        return None;
    }
    candidate.get(..3)
}

#[derive(Clone, Copy)]
struct SourceMarker {
    indent: usize,
    content_indent: usize,
    digit_start: Option<usize>,
    digit_len: usize,
    number: Option<u32>,
}

fn parse_source_marker(line: &str) -> Option<SourceMarker> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    let body = &line[indent..];
    if let Some(first) = body.bytes().next()
        && matches!(first, b'-' | b'+' | b'*')
        && body.as_bytes().get(1).is_some_and(u8::is_ascii_whitespace)
    {
        return Some(SourceMarker {
            indent,
            content_indent: indent + 2,
            digit_start: None,
            digit_len: 0,
            number: None,
        });
    }
    let digit_len = body.bytes().take_while(u8::is_ascii_digit).count();
    if !(1..=9).contains(&digit_len)
        || !matches!(body.as_bytes().get(digit_len), Some(b'.' | b')'))
        || !body
            .as_bytes()
            .get(digit_len + 1)
            .is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }
    Some(SourceMarker {
        indent,
        content_indent: indent + digit_len + 2,
        digit_start: Some(indent),
        digit_len,
        number: body[..digit_len].parse().ok(),
    })
}

/// marked allows a non-1 ordered list to interrupt an active list-item
/// paragraph when its marker lies within that item's content indent plus the
/// three-space continuation allowance. CommonMark parsers require 1 in that
/// position. Rewrite only that marker to `1.` plus same-width padding for
/// parsing; source offsets and original markers remain intact for rendering.
fn normalize_marked_ordered_interrupts(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    let mut active: Vec<SourceMarker> = Vec::new();
    let mut line_offset = 0usize;

    for line in source.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let Some(marker) = parse_source_marker(content) else {
            line_offset += line.len();
            continue;
        };
        let parent = active.iter().rposition(|candidate| {
            marker.indent >= candidate.content_indent
                && marker.indent <= candidate.content_indent + 3
        });

        let deepest_parent = parent.filter(|index| index + 1 == active.len());
        if marker.number.is_some_and(|number| number != 1)
            && let Some(parent_index) = deepest_parent
            && let Some(start) = marker.digit_start
        {
            let range = line_offset + start..line_offset + start + marker.digit_len + 1;
            bytes[range.clone()].fill(b' ');
            bytes[range.start] = b'1';
            bytes[range.start + 1] = b'.';
            active.truncate(parent_index + 1);
        } else if let Some(parent_index) = parent {
            active.truncate(parent_index + 1);
        } else {
            active.retain(|candidate| marker.indent > candidate.indent);
        }
        active.push(marker);
        line_offset += line.len();
    }

    String::from_utf8(bytes).expect("ASCII-only marker rewrite preserves UTF-8")
}

fn longest_word_width(text: &str) -> usize {
    strip_terminal_sequences(text)
        .split_whitespace()
        .map(visible_width)
        .max()
        .unwrap_or(1)
}

fn allocate_table_columns(natural: &[usize], minimum: &[usize], budget: usize) -> Vec<usize> {
    let natural_total: usize = natural.iter().sum();
    if natural_total <= budget {
        return natural.to_vec();
    }
    let mut widths = minimum.to_vec();
    while widths.iter().sum::<usize>() > budget {
        let Some((column, _)) = widths.iter().enumerate().max_by_key(|(_, width)| *width) else {
            break;
        };
        if widths[column] <= 1 {
            break;
        }
        widths[column] -= 1;
    }
    let mut remaining = budget.saturating_sub(widths.iter().sum());
    while remaining > 0 {
        let mut changed = false;
        for column in 0..widths.len() {
            if widths[column] < natural[column] {
                widths[column] += 1;
                remaining -= 1;
                changed = true;
                if remaining == 0 {
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
    widths
}

fn table_rule(widths: &[usize], left: char, junction: char, right: char) -> String {
    let mut line = left.to_string();
    for (index, width) in widths.iter().copied().enumerate() {
        line.push_str(&"─".repeat(width + 2));
        line.push(if index + 1 == widths.len() {
            right
        } else {
            junction
        });
    }
    line
}

#[cfg(test)]
mod tests {
    use super::{normalize_marked_ordered_interrupts, raw_node_source};

    #[test]
    fn marked_nested_ordered_interrupt_rewrite_is_width_preserving_and_general() {
        for (indent, rewritten) in [
            (3, false),
            (4, false),
            (5, true),
            (6, true),
            (7, true),
            (8, true),
            (9, false),
        ] {
            for marker in ["4", "12", "321"] {
                let source = format!(
                    "1. outer\n   - inner\n{}{}. deep\n2. tail",
                    " ".repeat(indent),
                    marker
                );
                let normalized = normalize_marked_ordered_interrupts(&source);
                assert_eq!(normalized.len(), source.len());
                let expected = if rewritten {
                    format!("{}1.{}", " ".repeat(indent), " ".repeat(marker.len() - 1))
                } else {
                    format!("{}{}.", " ".repeat(indent), marker)
                };
                assert!(normalized.lines().nth(2).unwrap().starts_with(&expected));
            }
        }
    }

    #[test]
    fn mdast_source_ranges_remain_utf8_boundaries_next_to_unicode() {
        fn assert_ranges(node: &markdown::mdast::Node, source: &str) {
            if node.position().is_some() {
                assert!(raw_node_source(node, source).is_some());
            }
            if let Some(children) = node.children() {
                for child in children {
                    assert_ranges(child, source);
                }
            }
        }

        for left in ["", "π", "文", "🙂", "e\u{301}"] {
            for right in ["", "é", "終", "🙂", "o\u{308}"] {
                let source = format!("{left}$x_{{é}}^2${right} &amp; <em>文</em>");
                let mut options = markdown::ParseOptions::gfm();
                options.constructs.math_text = true;
                let tree = markdown::to_mdast(&source, &options).expect("mdast");
                assert_ranges(&tree, &source);
            }
        }
    }
}
