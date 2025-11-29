//! # ratatui-md
//!
//! Markdown rendering for [ratatui](https://github.com/ratatui-org/ratatui) terminal UIs.
//!
//! This crate provides widgets and utilities for rendering Markdown documents
//! in terminal applications built with ratatui. It uses [MD4C](https://github.com/mity/md4c)
//! for fast, CommonMark-compliant parsing.
//!
//! ## Features
//!
//! - **Full Markdown Support**: Headings, emphasis, links, code blocks, lists, tables, etc.
//! - **GitHub Flavored Markdown**: Tables, task lists, strikethrough, autolinks
//! - **Customizable Themes**: Built-in themes or create your own
//! - **Interactive Widgets**: Scrolling, link navigation, heading jumping
//! - **Syntax Highlighting**: Optional code block highlighting via syntect
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use ratatui_md::Markdown;
//!
//! let markdown = "# Hello World\n\nThis is **bold** text.\n\n- Item 1\n- Item 2";
//!
//! // Create a widget
//! let widget = Markdown::new(markdown);
//!
//! // Render in your ratatui app
//! // frame.render_widget(widget, area);
//! ```
//!
//! ## Themes
//!
//! ```rust
//! use ratatui_md::{Markdown, Theme};
//!
//! // Use a built-in theme
//! let widget = Markdown::new("# Hello").theme(Theme::dark());
//!
//! // Or customize
//! use ratatui::style::{Color, Style};
//! let mut theme = Theme::default();
//! theme.heading1 = Style::new().fg(Color::Magenta);
//! let widget = Markdown::new("# Hello").theme(theme);
//! ```
//!
//! ## Interactive Viewing
//!
//! For scrollable, interactive markdown documents:
//!
//! ```rust
//! use ratatui_md::MarkdownView;
//!
//! let mut view = MarkdownView::new("# Doc\n\nLong content...");
//!
//! // Scroll
//! view.scroll_down(5);
//! view.scroll_up(2);
//!
//! // Navigate headings
//! let headings = view.headings();
//! view.scroll_to_heading(0);
//!
//! // Navigate links
//! view.select_next_link();
//! if let Some(link) = view.selected_link() {
//!     println!("Selected: {}", link.url);
//! }
//! ```
//!
//! ## Render Options
//!
//! ```rust
//! use ratatui_md::{Markdown, RenderOptions};
//! use md4c::ParserFlags;
//!
//! let options = RenderOptions::default()
//!     .with_width(80)
//!     .with_parser_flags(ParserFlags::github());
//!
//! let widget = Markdown::new("# Hello").options(options);
//! ```
//!
//! ## Direct Text Rendering
//!
//! For more control, render directly to ratatui `Text`:
//!
//! ```rust
//! use ratatui_md::{render, Theme, RenderOptions};
//!
//! let result = render("**bold**", &Theme::default(), &RenderOptions::default());
//! let text = result.text;
//! let links = result.links;
//! let headings = result.headings;
//! ```
//!
//! ## Syntax Highlighting
//!
//! Enable the `syntect` feature for code block highlighting:
//!
//! ```toml
//! [dependencies]
//! ratatui-md = { version = "0.1", features = ["syntect"] }
//! ```
//!
//! ```rust,ignore
//! use ratatui_md::SyntaxHighlighter;
//!
//! let highlighter = SyntaxHighlighter::new().theme("base16-ocean.dark");
//! let lines = highlighter.highlight("fn main() {}", "rust");
//! ```

pub mod highlight;
pub mod renderer;
pub mod theme;
pub mod widget;

// Re-export main types
pub use highlight::SyntaxHighlighter;
pub use renderer::{render, render_default, HeadingInfo, LinkInfo, RenderOptions, RenderedMarkdown};
pub use theme::Theme;
pub use widget::{Markdown, MarkdownSpan, MarkdownView, MarkdownViewWidget};

// Re-export md4c types that users might need
pub use md4c::ParserFlags;

/// Convenience function to render markdown to ratatui Text.
///
/// Uses default theme and options.
///
/// # Example
///
/// ```
/// let text = ratatui_md::to_text("# Hello **world**");
/// ```
pub fn to_text(markdown: &str) -> ratatui::text::Text<'static> {
    render_default(markdown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_text() {
        let text = to_text("# Hello");
        assert!(!text.lines.is_empty());
    }

    #[test]
    fn test_render_with_theme() {
        let theme = Theme::dark();
        let result = render("**bold**", &theme, &RenderOptions::default());
        assert!(!result.text.lines.is_empty());
    }

    #[test]
    fn test_markdown_widget() {
        let _widget = Markdown::new("# Test");
    }
}
