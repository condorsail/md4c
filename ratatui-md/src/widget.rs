//! Markdown widget for ratatui.
//!
//! Provides ready-to-use widgets for rendering markdown in terminal UIs.

use crate::renderer::{render, HeadingInfo, LinkInfo, RenderOptions, RenderedMarkdown, SearchMatch};
use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::Text;
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

/// A widget that renders markdown content.
///
/// This widget parses and renders markdown text using the configured theme
/// and options, then displays it as styled terminal text.
///
/// # Example
///
/// ```
/// use ratatui_md::{Markdown, Theme};
/// use ratatui::widgets::Block;
///
/// let markdown = Markdown::new("# Hello\n\nThis is **bold**.")
///     .theme(Theme::dark());
///
/// // Use in your ratatui render loop:
/// // frame.render_widget(markdown, area);
/// ```
#[derive(Clone)]
pub struct Markdown<'a> {
    content: &'a str,
    theme: Theme,
    options: RenderOptions,
    block: Option<Block<'a>>,
    wrap: bool,
    alignment: Alignment,
    scroll: (u16, u16),
}

impl<'a> Markdown<'a> {
    /// Create a new Markdown widget with the given content.
    pub fn new(content: &'a str) -> Self {
        Self {
            content,
            theme: Theme::default(),
            options: RenderOptions::default(),
            block: None,
            wrap: true,
            alignment: Alignment::Left,
            scroll: (0, 0),
        }
    }

    /// Set the theme for rendering.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Set the render options.
    pub fn options(mut self, options: RenderOptions) -> Self {
        self.options = options;
        self
    }

    /// Set a block to wrap the widget.
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Enable or disable line wrapping.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Set text alignment.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Set the scroll offset (vertical, horizontal).
    pub fn scroll(mut self, scroll: (u16, u16)) -> Self {
        self.scroll = scroll;
        self
    }

    /// Render the markdown and return the result.
    ///
    /// This is useful when you need access to link/heading metadata.
    pub fn render_to_text(&self) -> RenderedMarkdown<'static> {
        render(self.content, &self.theme, &self.options)
    }
}

impl Widget for Markdown<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut options = self.options;
        options.width = area.width as usize;

        let rendered = render(self.content, &self.theme, &options);

        let mut paragraph = Paragraph::new(rendered.text)
            .alignment(self.alignment)
            .scroll(self.scroll);

        if self.wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }

        if let Some(block) = self.block {
            paragraph = paragraph.block(block);
        }

        paragraph.render(area, buf);
    }
}

/// A stateful markdown widget that tracks scroll position and selection.
///
/// Use this when you need interactive features like scrolling, link
/// navigation, or heading jumping.
///
/// # Example
///
/// ```
/// use ratatui_md::{MarkdownView, Theme};
///
/// let mut view = MarkdownView::new("# Doc\n\nSome content...");
/// view.scroll_down(5);
/// ```
pub struct MarkdownView {
    content: String,
    theme: Theme,
    options: RenderOptions,
    rendered: Option<RenderedMarkdown<'static>>,
    scroll_offset: u16,
    selected_link: Option<usize>,
}

impl MarkdownView {
    /// Create a new markdown view.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            theme: Theme::default(),
            options: RenderOptions::default(),
            rendered: None,
            scroll_offset: 0,
            selected_link: None,
        }
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self.rendered = None; // Invalidate cache
        self
    }

    /// Set render options.
    pub fn options(mut self, options: RenderOptions) -> Self {
        self.options = options;
        self.rendered = None;
        self
    }

    /// Set the markdown content.
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.rendered = None;
        self.scroll_offset = 0;
        self.selected_link = None;
    }

    /// Get the current scroll offset.
    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    /// Set the scroll offset.
    pub fn set_scroll(&mut self, offset: u16) {
        self.scroll_offset = offset;
    }

    /// Scroll down by the given number of lines.
    pub fn scroll_down(&mut self, lines: u16) {
        self.ensure_rendered();
        let max_scroll = self.rendered.as_ref().map(|r| r.line_count).unwrap_or(0) as u16;
        self.scroll_offset = self.scroll_offset.saturating_add(lines).min(max_scroll);
    }

    /// Scroll up by the given number of lines.
    pub fn scroll_up(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll to the top.
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scroll to the bottom.
    pub fn scroll_to_bottom(&mut self) {
        self.ensure_rendered();
        if let Some(ref rendered) = self.rendered {
            self.scroll_offset = rendered.line_count.saturating_sub(1) as u16;
        }
    }

    /// Scroll to a specific heading by index.
    pub fn scroll_to_heading(&mut self, index: usize) {
        self.ensure_rendered();
        if let Some(ref rendered) = self.rendered {
            if let Some(heading) = rendered.headings.get(index) {
                self.scroll_offset = heading.line as u16;
            }
        }
    }

    /// Get all headings in the document.
    pub fn headings(&mut self) -> Vec<HeadingInfo> {
        self.ensure_rendered();
        self.rendered
            .as_ref()
            .map(|r| r.headings.clone())
            .unwrap_or_default()
    }

    /// Get all links in the document.
    pub fn links(&mut self) -> Vec<LinkInfo> {
        self.ensure_rendered();
        self.rendered
            .as_ref()
            .map(|r| r.links.clone())
            .unwrap_or_default()
    }

    /// Select the next link.
    pub fn select_next_link(&mut self) {
        self.ensure_rendered();
        let link_count = self
            .rendered
            .as_ref()
            .map(|r| r.links.len())
            .unwrap_or(0);

        if link_count == 0 {
            return;
        }

        self.selected_link = Some(match self.selected_link {
            Some(i) => (i + 1) % link_count,
            None => 0,
        });
    }

    /// Select the previous link.
    pub fn select_prev_link(&mut self) {
        self.ensure_rendered();
        let link_count = self
            .rendered
            .as_ref()
            .map(|r| r.links.len())
            .unwrap_or(0);

        if link_count == 0 {
            return;
        }

        self.selected_link = Some(match self.selected_link {
            Some(0) => link_count - 1,
            Some(i) => i - 1,
            None => link_count - 1,
        });
    }

    /// Get the currently selected link.
    pub fn selected_link(&mut self) -> Option<&LinkInfo> {
        self.ensure_rendered();
        self.selected_link
            .and_then(|i| self.rendered.as_ref()?.links.get(i))
    }

    /// Get the total line count.
    pub fn line_count(&mut self) -> usize {
        self.ensure_rendered();
        self.rendered.as_ref().map(|r| r.line_count).unwrap_or(0)
    }

    /// Get the rendered text.
    pub fn text(&mut self) -> &Text<'static> {
        self.ensure_rendered();
        &self.rendered.as_ref().unwrap().text
    }

    fn ensure_rendered(&mut self) {
        if self.rendered.is_none() {
            self.rendered = Some(render(&self.content, &self.theme, &self.options));
        }
    }

    /// Create a widget for rendering this view.
    pub fn widget(&mut self) -> MarkdownViewWidget<'_> {
        self.ensure_rendered();
        MarkdownViewWidget { view: self }
    }

    /// Set a search pattern to highlight.
    ///
    /// Matches are case-insensitive and will be highlighted in the rendered output.
    pub fn set_search(&mut self, pattern: impl Into<String>) {
        self.options.search_pattern = Some(pattern.into());
        self.rendered = None; // Invalidate cache
    }

    /// Clear the search pattern.
    pub fn clear_search(&mut self) {
        self.options.search_pattern = None;
        self.rendered = None;
    }

    /// Get the current search pattern.
    pub fn search_pattern(&self) -> Option<&str> {
        self.options.search_pattern.as_deref()
    }

    /// Get all search matches.
    pub fn search_matches(&mut self) -> Vec<SearchMatch> {
        self.ensure_rendered();
        self.rendered
            .as_ref()
            .map(|r| r.search_matches.clone())
            .unwrap_or_default()
    }

    /// Get the number of search matches.
    pub fn search_match_count(&mut self) -> usize {
        self.ensure_rendered();
        self.rendered
            .as_ref()
            .map(|r| r.search_matches.len())
            .unwrap_or(0)
    }

    /// Scroll to the next search match from the current scroll position.
    ///
    /// Returns the index of the match scrolled to, or None if no matches.
    pub fn scroll_to_next_match(&mut self) -> Option<usize> {
        self.ensure_rendered();
        let matches = self.rendered.as_ref()?.search_matches.clone();
        if matches.is_empty() {
            return None;
        }

        let current_line = self.scroll_offset as usize;

        // Find the next match after current position
        for (i, m) in matches.iter().enumerate() {
            if m.line > current_line {
                self.scroll_offset = m.line as u16;
                return Some(i);
            }
        }

        // Wrap around to first match
        self.scroll_offset = matches[0].line as u16;
        Some(0)
    }

    /// Scroll to the previous search match from the current scroll position.
    ///
    /// Returns the index of the match scrolled to, or None if no matches.
    pub fn scroll_to_prev_match(&mut self) -> Option<usize> {
        self.ensure_rendered();
        let matches = self.rendered.as_ref()?.search_matches.clone();
        if matches.is_empty() {
            return None;
        }

        let current_line = self.scroll_offset as usize;

        // Find the previous match before current position
        for (i, m) in matches.iter().enumerate().rev() {
            if m.line < current_line {
                self.scroll_offset = m.line as u16;
                return Some(i);
            }
        }

        // Wrap around to last match
        let last = matches.len() - 1;
        self.scroll_offset = matches[last].line as u16;
        Some(last)
    }

    /// Scroll to a specific search match by index.
    pub fn scroll_to_match(&mut self, index: usize) {
        self.ensure_rendered();
        if let Some(ref rendered) = self.rendered {
            if let Some(m) = rendered.search_matches.get(index) {
                self.scroll_offset = m.line as u16;
            }
        }
    }
}

/// Widget wrapper for MarkdownView.
pub struct MarkdownViewWidget<'a> {
    view: &'a mut MarkdownView,
}

impl Widget for MarkdownViewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = self.view.rendered.as_ref().map(|r| r.text.clone()).unwrap_or_default();

        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((self.view.scroll_offset, 0))
            .render(area, buf);
    }
}

/// A simple markdown paragraph widget.
///
/// This is a convenience widget for rendering a single markdown string
/// inline within other widgets.
pub struct MarkdownSpan<'a> {
    content: &'a str,
    theme: Theme,
    style: Style,
}

impl<'a> MarkdownSpan<'a> {
    /// Create a new markdown span.
    pub fn new(content: &'a str) -> Self {
        Self {
            content,
            theme: Theme::default(),
            style: Style::default(),
        }
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Set additional style to apply.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Convert to ratatui Text.
    pub fn to_text(&self) -> Text<'static> {
        let options = RenderOptions {
            paragraph_space: false,
            heading_space: false,
            ..Default::default()
        };
        render(self.content, &self.theme, &options).text
    }
}

impl Widget for MarkdownSpan<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.to_text())
            .style(self.style)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_widget() {
        let md = Markdown::new("# Test\n\nHello");
        let rendered = md.render_to_text();
        assert!(!rendered.text.lines.is_empty());
    }

    #[test]
    fn test_markdown_view_scrolling() {
        let mut view = MarkdownView::new("# Line 1\n\n# Line 2\n\n# Line 3");
        view.scroll_down(2);
        assert_eq!(view.scroll_offset(), 2);
        view.scroll_up(1);
        assert_eq!(view.scroll_offset(), 1);
        view.scroll_to_top();
        assert_eq!(view.scroll_offset(), 0);
    }

    #[test]
    fn test_markdown_view_headings() {
        let mut view = MarkdownView::new("# H1\n\n## H2\n\n### H3");
        let headings = view.headings();
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[2].level, 3);
    }

    #[test]
    fn test_markdown_view_links() {
        let mut view = MarkdownView::new("[a](http://a.com) and [b](http://b.com)");
        let links = view.links();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_markdown_view_search() {
        let mut view = MarkdownView::new("Hello world\n\nHello again\n\nGoodbye");
        view.set_search("hello");
        let matches = view.search_matches();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_markdown_view_search_navigation() {
        let mut view = MarkdownView::new("Hello\n\nworld\n\nHello\n\nagain");
        view.set_search("hello");

        // Jump to first match
        let idx = view.scroll_to_next_match();
        assert!(idx.is_some());

        // Clear and check
        view.clear_search();
        assert!(view.search_pattern().is_none());
    }
}
