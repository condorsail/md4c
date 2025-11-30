//! Core markdown-to-ratatui renderer.
//!
//! Converts parsed markdown into ratatui `Text` structures.

use crate::theme::Theme;
#[cfg(feature = "syntect")]
use crate::highlight::SyntaxHighlighter;
use md4c::{
    parse, Alignment, Block, BlockType, CodeBlockDetail, HeadingDetail, ImageDetail, LinkDetail,
    ListItemDetail, OrderedListDetail, ParserFlags, ParserHandler, Span, SpanType, TableCellDetail,
    TableDetail, TaskState, TextType, UnorderedListDetail, WikiLinkDetail,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span as RSpan, Text};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Render options for the markdown renderer.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Maximum width for wrapping (0 = no wrapping)
    pub width: usize,
    /// Parser flags for MD4C
    pub parser_flags: ParserFlags,
    /// Whether to include a blank line after headings
    pub heading_space: bool,
    /// Whether to include a blank line after paragraphs
    pub paragraph_space: bool,
    /// Whether to include a blank line after code blocks
    pub code_block_space: bool,
    /// Whether to include a blank line after lists
    pub list_space: bool,
    /// Search pattern to highlight (case-insensitive)
    pub search_pattern: Option<String>,
    /// Style for search highlights
    pub search_highlight_style: Style,
    /// Whether to use syntax highlighting for code blocks
    pub syntax_highlighting: bool,
    /// Syntax highlighting theme name (if syntect feature enabled)
    pub syntax_theme: Option<String>,
}

impl RenderOptions {
    /// Create a new RenderOptions with defaults.
    pub fn new() -> Self {
        Self {
            width: 0,
            parser_flags: ParserFlags::github(),
            heading_space: true,
            paragraph_space: true,
            code_block_space: true,
            list_space: true,
            search_pattern: None,
            search_highlight_style: Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            syntax_highlighting: true,
            syntax_theme: None,
        }
    }

    /// Create options with CommonMark parsing.
    pub fn commonmark() -> Self {
        Self {
            parser_flags: ParserFlags::commonmark(),
            ..Self::new()
        }
    }

    /// Create options with GitHub Flavored Markdown parsing.
    pub fn github() -> Self {
        Self {
            parser_flags: ParserFlags::github(),
            ..Self::new()
        }
    }

    /// Set the maximum width for line wrapping.
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Set parser flags.
    pub fn with_parser_flags(mut self, flags: ParserFlags) -> Self {
        self.parser_flags = flags;
        self
    }

    /// Set a search pattern to highlight.
    pub fn with_search(mut self, pattern: impl Into<String>) -> Self {
        self.search_pattern = Some(pattern.into());
        self
    }

    /// Clear the search pattern.
    pub fn clear_search(mut self) -> Self {
        self.search_pattern = None;
        self
    }

    /// Set the search highlight style.
    pub fn with_search_style(mut self, style: Style) -> Self {
        self.search_highlight_style = style;
        self
    }

    /// Enable or disable syntax highlighting.
    pub fn with_syntax_highlighting(mut self, enabled: bool) -> Self {
        self.syntax_highlighting = enabled;
        self
    }

    /// Set the syntax highlighting theme.
    pub fn with_syntax_theme(mut self, theme: impl Into<String>) -> Self {
        self.syntax_theme = Some(theme.into());
        self
    }
}

/// A rendered markdown document.
///
/// Contains the converted ratatui `Text` along with metadata about
/// links, headings, and other interactive elements.
#[derive(Debug, Clone)]
pub struct RenderedMarkdown<'a> {
    /// The rendered text content
    pub text: Text<'a>,
    /// Links found in the document: (line_index, start_col, end_col, url)
    pub links: Vec<LinkInfo>,
    /// Headings found in the document: (line_index, level, text)
    pub headings: Vec<HeadingInfo>,
    /// Total line count
    pub line_count: usize,
    /// Search match locations: (line_index, start_col, end_col)
    pub search_matches: Vec<SearchMatch>,
}

/// Information about a link in the rendered document.
#[derive(Debug, Clone)]
pub struct LinkInfo {
    /// Line index where the link appears
    pub line: usize,
    /// URL or target of the link
    pub url: String,
    /// Display text of the link
    pub text: String,
    /// Whether this is an autolink
    pub is_autolink: bool,
}

/// Information about a heading in the rendered document.
#[derive(Debug, Clone)]
pub struct HeadingInfo {
    /// Line index where the heading appears
    pub line: usize,
    /// Heading level (1-6)
    pub level: u8,
    /// Heading text content
    pub text: String,
}

/// Information about a search match.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Line index
    pub line: usize,
    /// Start column (character index)
    pub start: usize,
    /// End column (character index)
    pub end: usize,
}

/// Word wrap a line of spans to fit within the given width.
fn wrap_line(spans: Vec<RSpan<'static>>, max_width: usize, indent: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return vec![Line::from(spans)];
    }

    let mut result = Vec::new();
    let mut current_line: Vec<RSpan<'static>> = Vec::new();
    let mut current_width = 0;
    let indent_str = " ".repeat(indent);

    for span in spans {
        let text = span.content.to_string();
        let style = span.style;
        let mut remaining = text.as_str();

        while !remaining.is_empty() {
            let span_width = remaining.width();

            if current_width + span_width <= max_width {
                // Fits on current line
                current_line.push(RSpan::styled(remaining.to_string(), style));
                current_width += span_width;
                break;
            }

            // Need to wrap
            let available = max_width.saturating_sub(current_width);
            if available == 0 {
                // Start new line
                if !current_line.is_empty() {
                    result.push(Line::from(std::mem::take(&mut current_line)));
                }
                current_line.push(RSpan::raw(indent_str.clone()));
                current_width = indent;
                continue;
            }

            // Find wrap point
            let mut wrap_at = 0;
            let mut width_so_far = 0;
            let mut last_space = None;

            for (i, c) in remaining.char_indices() {
                let char_width = c.width().unwrap_or(1);
                if width_so_far + char_width > available {
                    break;
                }
                width_so_far += char_width;
                wrap_at = i + c.len_utf8();
                if c.is_whitespace() {
                    last_space = Some(i);
                }
            }

            // Prefer wrapping at word boundary
            let break_at = if let Some(space_pos) = last_space {
                if space_pos > 0 {
                    space_pos
                } else {
                    wrap_at
                }
            } else if wrap_at > 0 {
                wrap_at
            } else {
                // Can't fit even one character, force break
                remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1)
            };

            let (before, after) = remaining.split_at(break_at);
            if !before.is_empty() {
                current_line.push(RSpan::styled(before.to_string(), style));
            }

            // Start new line
            result.push(Line::from(std::mem::take(&mut current_line)));
            current_line.push(RSpan::raw(indent_str.clone()));
            current_width = indent;

            // Skip leading whitespace on new line
            remaining = after.trim_start();
        }
    }

    if !current_line.is_empty() {
        result.push(Line::from(current_line));
    }

    if result.is_empty() {
        result.push(Line::from(vec![]));
    }

    result
}

/// Apply search highlighting to spans.
fn highlight_search(spans: Vec<RSpan<'static>>, pattern: &str, style: Style) -> (Vec<RSpan<'static>>, Vec<(usize, usize)>) {
    if pattern.is_empty() {
        return (spans, vec![]);
    }

    let pattern_lower = pattern.to_lowercase();
    let mut result = Vec::new();
    let mut matches = Vec::new();
    let mut char_offset = 0;

    for span in spans {
        let text = span.content.to_string();
        let text_lower = text.to_lowercase();
        let base_style = span.style;

        let mut last_end = 0;
        for (match_start, _) in text_lower.match_indices(&pattern_lower) {
            let match_end = match_start + pattern.len();

            // Add non-matching part before
            if match_start > last_end {
                result.push(RSpan::styled(text[last_end..match_start].to_string(), base_style));
            }

            // Add matching part with highlight
            result.push(RSpan::styled(text[match_start..match_end].to_string(), style));
            matches.push((char_offset + match_start, char_offset + match_end));

            last_end = match_end;
        }

        // Add remaining non-matching part
        if last_end < text.len() {
            result.push(RSpan::styled(text[last_end..].to_string(), base_style));
        }

        char_offset += text.len();
    }

    (result, matches)
}

/// Internal state for the renderer.
struct RendererState<'a> {
    theme: &'a Theme,
    options: &'a RenderOptions,
    #[cfg(feature = "syntect")]
    highlighter: Option<SyntaxHighlighter>,

    // Output
    lines: Vec<Line<'static>>,
    current_spans: Vec<RSpan<'static>>,
    links: Vec<LinkInfo>,
    headings: Vec<HeadingInfo>,
    search_matches: Vec<SearchMatch>,

    // Style stack for nested formatting
    style_stack: Vec<Style>,

    // Block context
    in_heading: Option<u8>,
    in_blockquote: bool,
    in_code_block: bool,
    code_block_lang: String,
    code_block_content: String,
    in_list: bool,
    list_depth: usize,
    list_counters: Vec<u32>,
    list_is_ordered: Vec<bool>,
    current_task_state: Option<TaskState>,

    // Table state
    in_table: bool,
    table_columns: usize,
    table_alignments: Vec<Alignment>,
    table_rows: Vec<Vec<Vec<RSpan<'static>>>>,
    current_table_row: Vec<Vec<RSpan<'static>>>,
    current_table_cell: Vec<RSpan<'static>>,
    in_table_header: bool,

    // Link tracking
    current_link: Option<LinkDetail>,
    current_link_text: String,

    // Paragraph tracking
    pending_newline: bool,
}

impl<'a> RendererState<'a> {
    fn new(theme: &'a Theme, options: &'a RenderOptions) -> Self {
        #[cfg(feature = "syntect")]
        let highlighter = if options.syntax_highlighting {
            let mut h = SyntaxHighlighter::new();
            if let Some(ref theme_name) = options.syntax_theme {
                h = h.theme(theme_name);
            }
            Some(h)
        } else {
            None
        };

        Self {
            theme,
            options,
            #[cfg(feature = "syntect")]
            highlighter,
            lines: Vec::new(),
            current_spans: Vec::new(),
            links: Vec::new(),
            headings: Vec::new(),
            search_matches: Vec::new(),
            style_stack: vec![theme.text],
            in_heading: None,
            in_blockquote: false,
            in_code_block: false,
            code_block_lang: String::new(),
            code_block_content: String::new(),
            in_list: false,
            list_depth: 0,
            list_counters: Vec::new(),
            list_is_ordered: Vec::new(),
            current_task_state: None,
            in_table: false,
            table_columns: 0,
            table_alignments: Vec::new(),
            table_rows: Vec::new(),
            current_table_row: Vec::new(),
            current_table_cell: Vec::new(),
            in_table_header: false,
            current_link: None,
            current_link_text: String::new(),
            pending_newline: false,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or(self.theme.text)
    }

    fn push_style(&mut self, style: Style) {
        let current = self.current_style();
        let merged = current.patch(style);
        self.style_stack.push(merged);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        // Handle code block content collection
        if self.in_code_block {
            self.code_block_content.push_str(text);
            return;
        }

        if self.in_table {
            self.current_table_cell
                .push(RSpan::styled(text.to_string(), self.current_style()));
            return;
        }

        if self.current_link.is_some() {
            self.current_link_text.push_str(text);
        }

        self.current_spans
            .push(RSpan::styled(text.to_string(), self.current_style()));
    }

    fn finish_line(&mut self) {
        self.finish_line_with_wrap(0);
    }

    fn finish_line_with_wrap(&mut self, indent: usize) {
        if self.in_table || self.in_code_block {
            return;
        }

        let mut spans = std::mem::take(&mut self.current_spans);

        // Add blockquote prefix if needed
        if self.in_blockquote && !spans.is_empty() {
            spans.insert(
                0,
                RSpan::styled(
                    self.theme.blockquote_prefix.to_string(),
                    self.theme.blockquote_marker,
                ),
            );
        }

        if spans.is_empty() && !self.pending_newline {
            return;
        }

        // Apply search highlighting
        if let Some(ref pattern) = self.options.search_pattern {
            let line_idx = self.lines.len();
            let (highlighted_spans, matches) = highlight_search(spans, pattern, self.options.search_highlight_style);
            spans = highlighted_spans;
            for (start, end) in matches {
                self.search_matches.push(SearchMatch {
                    line: line_idx,
                    start,
                    end,
                });
            }
        }

        // Apply word wrapping
        if self.options.width > 0 && !spans.is_empty() {
            let wrapped = wrap_line(spans, self.options.width, indent);
            self.lines.extend(wrapped);
        } else {
            self.lines.push(Line::from(spans));
        }

        self.pending_newline = false;
    }

    fn add_blank_line(&mut self) {
        self.finish_line();
        self.lines.push(Line::from(vec![]));
    }

    fn get_list_prefix(&mut self) -> String {
        let indent = " ".repeat(self.list_depth.saturating_sub(1) * self.theme.list_indent);

        if let Some(task_state) = self.current_task_state.take() {
            let marker = match task_state {
                TaskState::Checked => self.theme.task_checked_char,
                TaskState::Unchecked => self.theme.task_unchecked_char,
                TaskState::NotTask => self.theme.bullet_char,
            };
            return format!("{}{} ", indent, marker);
        }

        if self.list_depth == 0 {
            return String::new();
        }

        let idx = self.list_depth - 1;
        if idx < self.list_is_ordered.len() && self.list_is_ordered[idx] {
            let num = self.list_counters.get(idx).copied().unwrap_or(1);
            format!("{}{}. ", indent, num)
        } else {
            format!("{}{} ", indent, self.theme.bullet_char)
        }
    }

    fn render_horizontal_rule(&mut self) {
        let width = if self.options.width > 0 {
            self.options.width
        } else {
            40
        };
        let hr = self.theme.hr_char.to_string().repeat(width);
        self.lines
            .push(Line::from(vec![RSpan::styled(hr, self.theme.horizontal_rule)]));
    }

    fn render_code_block(&mut self) {
        let content = std::mem::take(&mut self.code_block_content);
        #[allow(unused_variables)]
        let lang = std::mem::take(&mut self.code_block_lang);

        #[cfg(feature = "syntect")]
        if let Some(ref highlighter) = self.highlighter {
            if !lang.is_empty() {
                let highlighted_lines = highlighter.highlight(&content, &lang);
                self.lines.extend(highlighted_lines);
                return;
            }
        }

        // Fallback: render without highlighting
        for line in content.lines() {
            let mut spans = vec![RSpan::styled(line.to_string(), self.theme.code_block)];

            // Apply search highlighting to code
            if let Some(ref pattern) = self.options.search_pattern {
                let line_idx = self.lines.len();
                let (highlighted, matches) = highlight_search(spans, pattern, self.options.search_highlight_style);
                spans = highlighted;
                for (start, end) in matches {
                    self.search_matches.push(SearchMatch {
                        line: line_idx,
                        start,
                        end,
                    });
                }
            }

            self.lines.push(Line::from(spans));
        }
    }

    fn render_table(&mut self) {
        if self.table_rows.is_empty() {
            return;
        }

        let mut col_widths: Vec<usize> = vec![0; self.table_columns];
        for row in &self.table_rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    let cell_width: usize = cell.iter().map(|s| s.content.width()).sum();
                    col_widths[i] = col_widths[i].max(cell_width);
                }
            }
        }

        for w in &mut col_widths {
            *w = (*w).max(3);
        }

        // Top border
        let top_border: String = col_widths
            .iter()
            .map(|w| "─".repeat(*w + 2))
            .collect::<Vec<_>>()
            .join("┬");
        self.lines.push(Line::from(vec![RSpan::styled(
            format!("┌{}┐", top_border),
            self.theme.table_border,
        )]));

        // Rows
        for (row_idx, row) in self.table_rows.iter().enumerate() {
            let mut line_spans = vec![RSpan::styled("│ ".to_string(), self.theme.table_border)];

            for (col_idx, cell) in row.iter().enumerate() {
                let cell_text: String = cell.iter().map(|s| s.content.to_string()).collect();
                let width = col_widths.get(col_idx).copied().unwrap_or(3);
                let align = self.table_alignments.get(col_idx).copied().unwrap_or(Alignment::Default);

                let padded = match align {
                    Alignment::Center => format!("{:^width$}", cell_text, width = width),
                    Alignment::Right => format!("{:>width$}", cell_text, width = width),
                    _ => format!("{:<width$}", cell_text, width = width),
                };

                let style = if row_idx == 0 {
                    self.theme.table_header
                } else {
                    self.theme.table_cell
                };

                line_spans.push(RSpan::styled(padded, style));
                line_spans.push(RSpan::styled(" │ ".to_string(), self.theme.table_border));
            }

            self.lines.push(Line::from(line_spans));

            if row_idx == 0 {
                let sep: String = col_widths
                    .iter()
                    .enumerate()
                    .map(|(i, w)| {
                        let align = self.table_alignments.get(i).copied().unwrap_or(Alignment::Default);
                        match align {
                            Alignment::Left => format!(":{}─", "─".repeat(*w)),
                            Alignment::Right => format!("{}─:", "─".repeat(*w)),
                            Alignment::Center => format!(":{}:", "─".repeat(*w)),
                            _ => "─".repeat(*w + 2),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("┼");
                self.lines.push(Line::from(vec![RSpan::styled(
                    format!("├{}┤", sep),
                    self.theme.table_border,
                )]));
            }
        }

        // Bottom border
        let bottom_border: String = col_widths
            .iter()
            .map(|w| "─".repeat(*w + 2))
            .collect::<Vec<_>>()
            .join("┴");
        self.lines.push(Line::from(vec![RSpan::styled(
            format!("└{}┘", bottom_border),
            self.theme.table_border,
        )]));

        self.table_rows.clear();
        self.table_columns = 0;
        self.table_alignments.clear();
    }
}

impl ParserHandler for RendererState<'_> {
    fn enter_block(&mut self, block: Block) -> bool {
        match block {
            Block::Document => {}

            Block::Paragraph => {
                if self.in_list && self.list_depth > 0 {
                    let prefix = self.get_list_prefix();
                    if !prefix.is_empty() {
                        let style = if self.list_is_ordered.last().copied().unwrap_or(false) {
                            self.theme.list_number
                        } else {
                            self.theme.list_bullet
                        };
                        self.current_spans.push(RSpan::styled(prefix, style));
                    }
                }
            }

            Block::Heading(HeadingDetail { level }) => {
                self.in_heading = Some(level);
                self.push_style(self.theme.heading_style(level));

                let prefix = "#".repeat(level as usize);
                self.current_spans.push(RSpan::styled(
                    format!("{} ", prefix),
                    self.theme.heading_style(level),
                ));
            }

            Block::Quote => {
                self.in_blockquote = true;
                self.push_style(self.theme.blockquote);
            }

            Block::Code(CodeBlockDetail { lang, .. }) => {
                self.in_code_block = true;
                self.code_block_lang = lang.clone();
                self.code_block_content.clear();

                if !lang.is_empty() {
                    self.lines.push(Line::from(vec![RSpan::styled(
                        format!("{}:", lang),
                        self.theme.code_block_info,
                    )]));
                }

                self.push_style(self.theme.code_block);
            }

            Block::UnorderedList(UnorderedListDetail { .. }) => {
                self.in_list = true;
                self.list_depth += 1;
                self.list_is_ordered.push(false);
                self.list_counters.push(1);
            }

            Block::OrderedList(OrderedListDetail { start, .. }) => {
                self.in_list = true;
                self.list_depth += 1;
                self.list_is_ordered.push(true);
                self.list_counters.push(start);
            }

            Block::ListItem(ListItemDetail { task_state }) => {
                if task_state != TaskState::NotTask {
                    self.current_task_state = Some(task_state);
                }
            }

            Block::HorizontalRule => {
                self.render_horizontal_rule();
            }

            Block::Html => {
                self.push_style(self.theme.raw_html);
            }

            Block::Table(TableDetail { column_count, .. }) => {
                self.in_table = true;
                self.table_columns = column_count as usize;
                self.table_alignments = vec![Alignment::Default; column_count as usize];
            }

            Block::TableHead => {
                self.in_table_header = true;
            }

            Block::TableBody => {
                self.in_table_header = false;
            }

            Block::TableRow => {
                self.current_table_row = Vec::new();
            }

            Block::TableHeaderCell(TableCellDetail { alignment }) | Block::TableCell(TableCellDetail { alignment }) => {
                self.current_table_cell = Vec::new();
                let col_idx = self.current_table_row.len();
                if col_idx < self.table_alignments.len() {
                    self.table_alignments[col_idx] = alignment;
                }
            }
        }
        true
    }

    fn leave_block(&mut self, block_type: BlockType) -> bool {
        match block_type {
            BlockType::Document => {}

            BlockType::Paragraph => {
                let indent = if self.in_list { self.list_depth * self.theme.list_indent } else { 0 };
                self.finish_line_with_wrap(indent);
                if self.options.paragraph_space && !self.in_list {
                    self.add_blank_line();
                }
            }

            BlockType::Heading => {
                if let Some(level) = self.in_heading.take() {
                    let text: String = self.current_spans.iter().map(|s| s.content.to_string()).collect();
                    self.headings.push(HeadingInfo {
                        line: self.lines.len(),
                        level,
                        text: text.trim_start_matches(['#', ' ']).to_string(),
                    });
                }
                self.finish_line();
                self.pop_style();
                if self.options.heading_space {
                    self.add_blank_line();
                }
            }

            BlockType::Quote => {
                self.finish_line();
                self.in_blockquote = false;
                self.pop_style();
                self.add_blank_line();
            }

            BlockType::Code => {
                self.render_code_block();
                self.in_code_block = false;
                self.pop_style();
                if self.options.code_block_space {
                    self.add_blank_line();
                }
            }

            BlockType::UnorderedList | BlockType::OrderedList => {
                self.list_depth = self.list_depth.saturating_sub(1);
                self.list_is_ordered.pop();
                self.list_counters.pop();
                if self.list_depth == 0 {
                    self.in_list = false;
                    if self.options.list_space {
                        self.add_blank_line();
                    }
                }
            }

            BlockType::ListItem => {
                self.finish_line();
                if let Some(counter) = self.list_counters.last_mut() {
                    *counter += 1;
                }
            }

            BlockType::HorizontalRule => {
                self.add_blank_line();
            }

            BlockType::Html => {
                self.finish_line();
                self.pop_style();
            }

            BlockType::Table => {
                self.render_table();
                self.in_table = false;
                self.add_blank_line();
            }

            BlockType::TableHead | BlockType::TableBody => {}

            BlockType::TableRow => {
                self.table_rows.push(std::mem::take(&mut self.current_table_row));
            }

            BlockType::TableHeaderCell | BlockType::TableCell => {
                self.current_table_row.push(std::mem::take(&mut self.current_table_cell));
            }

            _ => {}
        }
        true
    }

    fn enter_span(&mut self, span: Span) -> bool {
        match span {
            Span::Emphasis => {
                self.push_style(self.theme.emphasis);
            }
            Span::Strong => {
                self.push_style(self.theme.strong);
            }
            Span::Strikethrough => {
                self.push_style(self.theme.strikethrough);
            }
            Span::Underline => {
                self.push_style(self.theme.underline);
            }
            Span::Code => {
                self.push_style(self.theme.code_inline);
            }
            Span::Link(detail) => {
                self.current_link = Some(detail);
                self.current_link_text.clear();
                self.push_style(self.theme.link);
            }
            Span::Image(ImageDetail { src, title }) => {
                self.push_style(self.theme.image);
                let alt_text = if title.is_empty() { "image" } else { &title };
                self.push_text(&format!("[{}]", alt_text));
                if !src.is_empty() {
                    self.current_spans
                        .push(RSpan::styled(format!("({})", src), self.theme.link_url));
                }
            }
            Span::LatexMath | Span::LatexMathDisplay => {
                self.push_style(self.theme.latex_math);
            }
            Span::WikiLink(WikiLinkDetail { target }) => {
                self.push_style(self.theme.wiki_link);
                self.links.push(LinkInfo {
                    line: self.lines.len(),
                    url: target.clone(),
                    text: target,
                    is_autolink: false,
                });
            }
        }
        true
    }

    fn leave_span(&mut self, span_type: SpanType) -> bool {
        match span_type {
            SpanType::Link => {
                if let Some(detail) = self.current_link.take() {
                    self.links.push(LinkInfo {
                        line: self.lines.len(),
                        url: detail.href.clone(),
                        text: std::mem::take(&mut self.current_link_text),
                        is_autolink: detail.is_autolink,
                    });

                    if self.theme.show_link_urls && !detail.href.is_empty() {
                        self.pop_style();
                        self.current_spans
                            .push(RSpan::styled(format!(" ({})", detail.href), self.theme.link_url));
                        return true;
                    }
                }
                self.pop_style();
            }
            SpanType::Image => {
                self.pop_style();
            }
            SpanType::Emphasis
            | SpanType::Strong
            | SpanType::Strikethrough
            | SpanType::Underline
            | SpanType::Code
            | SpanType::LatexMath
            | SpanType::LatexMathDisplay
            | SpanType::WikiLink => {
                self.pop_style();
            }
            _ => {
                self.pop_style();
            }
        }
        true
    }

    fn text(&mut self, text_type: TextType, text: &str) -> bool {
        match text_type {
            TextType::Normal | TextType::Code | TextType::LatexMath => {
                self.push_text(text);
            }
            TextType::HardBreak => {
                if self.in_code_block {
                    self.code_block_content.push('\n');
                } else {
                    self.finish_line();
                    if self.in_list && self.list_depth > 0 {
                        let indent = " ".repeat(self.list_depth * self.theme.list_indent);
                        self.current_spans.push(RSpan::raw(indent));
                    }
                }
            }
            TextType::SoftBreak => {
                if self.in_code_block {
                    self.code_block_content.push('\n');
                } else {
                    self.push_text(" ");
                }
            }
            TextType::Entity => {
                self.current_spans
                    .push(RSpan::styled(text.to_string(), self.theme.html_entity));
            }
            TextType::Html => {
                self.current_spans
                    .push(RSpan::styled(text.to_string(), self.theme.raw_html));
            }
            TextType::NullChar => {
                self.push_text("\u{FFFD}");
            }
            _ => {
                self.push_text(text);
            }
        }
        true
    }
}

/// Render markdown to ratatui Text.
///
/// # Arguments
/// * `markdown` - The markdown source text
/// * `theme` - The theme to use for styling
/// * `options` - Rendering options
///
/// # Returns
/// A `RenderedMarkdown` containing the styled text and metadata.
///
/// # Example
///
/// ```
/// use ratatui_md::{render, Theme, RenderOptions};
///
/// let markdown = "# Hello\n\nThis is **bold** text.";
/// let result = render(markdown, &Theme::default(), &RenderOptions::default());
/// ```
pub fn render<'a>(
    markdown: &str,
    theme: &Theme,
    options: &RenderOptions,
) -> RenderedMarkdown<'a> {
    let mut state = RendererState::new(theme, options);

    let _ = parse(markdown, options.parser_flags, &mut state);

    state.finish_line();

    let line_count = state.lines.len();

    RenderedMarkdown {
        text: Text::from(state.lines),
        links: state.links,
        headings: state.headings,
        line_count,
        search_matches: state.search_matches,
    }
}

/// Render markdown to ratatui Text with default options.
///
/// Convenience function using default theme and options.
pub fn render_default(markdown: &str) -> Text<'static> {
    render(markdown, &Theme::default(), &RenderOptions::default()).text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rendering() {
        let result = render("Hello **world**", &Theme::default(), &RenderOptions::default());
        assert!(!result.text.lines.is_empty());
    }

    #[test]
    fn test_heading() {
        let result = render("# Title", &Theme::default(), &RenderOptions::default());
        assert_eq!(result.headings.len(), 1);
        assert_eq!(result.headings[0].level, 1);
        assert_eq!(result.headings[0].text, "Title");
    }

    #[test]
    fn test_links() {
        let result = render(
            "[click me](https://example.com)",
            &Theme::default(),
            &RenderOptions::default(),
        );
        assert_eq!(result.links.len(), 1);
        assert_eq!(result.links[0].url, "https://example.com");
        assert_eq!(result.links[0].text, "click me");
    }

    #[test]
    fn test_list() {
        let result = render("- item 1\n- item 2", &Theme::default(), &RenderOptions::default());
        assert!(result.text.lines.len() >= 2);
    }

    #[test]
    fn test_code_block() {
        let result = render("```rust\nfn main() {}\n```", &Theme::default(), &RenderOptions::default());
        assert!(result.text.lines.len() >= 1);
    }

    #[test]
    fn test_table() {
        let result = render(
            "| A | B |\n|---|---|\n| 1 | 2 |",
            &Theme::default(),
            &RenderOptions::github(),
        );
        assert!(result.text.lines.len() >= 3);
    }

    #[test]
    fn test_word_wrap() {
        let long_text = "This is a very long line that should be wrapped when the width is set.";
        let options = RenderOptions::default().with_width(20);
        let result = render(long_text, &Theme::default(), &options);
        // Should have multiple lines due to wrapping
        assert!(result.text.lines.len() > 1);
    }

    #[test]
    fn test_search_highlighting() {
        let options = RenderOptions::default().with_search("hello");
        let result = render("Hello world, hello there!", &Theme::default(), &options);
        // Should find 2 matches (case-insensitive)
        assert_eq!(result.search_matches.len(), 2);
    }

    #[test]
    fn test_search_no_matches() {
        let options = RenderOptions::default().with_search("xyz");
        let result = render("Hello world", &Theme::default(), &options);
        assert_eq!(result.search_matches.len(), 0);
    }
}
