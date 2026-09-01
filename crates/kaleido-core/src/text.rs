//! Text objects — rich-text layout (type / layout mode).
//!
//! A [`TextObject`] holds a sequence of rich-text runs so a single text
//! block can mix fonts, sizes and colors.  Glyph layout and rasterization
//! are handled by the (service-layer) text engine; the data model only
//! stores the logical content and paragraph frame.

use super::types::{Color, ResourceId, Size};

/// Horizontal alignment of text within its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

/// A rich-text run: a span of `text[start..end]` with uniform style.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextRun {
    /// Byte range into the owning [`TextObject::text`].
    pub start: usize,
    pub end: usize,
    pub font: ResourceId,
    pub size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
}

/// Optional fixed-size text frame.  `None` = free-flowing (auto-width).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextFrame {
    pub size: Size,
}

/// A text object attached to a node (type / layout mode).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextObject {
    /// The full UTF-8 text; runs index into it.
    pub text: String,
    /// Rich-text runs (must be sorted, non-overlapping, within bounds).
    pub runs: Vec<TextRun>,
    /// Default font for the object (used when a run has no explicit font).
    pub font: ResourceId,
    /// Default font size.
    pub size: f32,
    pub align: TextAlign,
    pub frame: Option<TextFrame>,
}

impl TextObject {
    /// Creates an empty text object with a single empty run.
    #[inline]
    pub fn new(font: ResourceId, size: f32) -> Self {
        Self {
            text: String::new(),
            runs: Vec::new(),
            font,
            size,
            align: TextAlign::Left,
            frame: None,
        }
    }

    /// Creates a text object with plain (single-run) text.
    #[inline]
    pub fn new_with_text(font: ResourceId, size: f32, text: impl Into<String>) -> Self {
        let mut obj = Self::new(font, size);
        obj.set_plain_text(text);
        obj
    }

    /// Sets plain text with a single run covering the whole string.
    pub fn set_plain_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        let end = text.len();
        self.runs.clear();
        if end > 0 {
            self.runs.push(TextRun {
                start: 0,
                end,
                font: self.font,
                size: self.size,
                color: Color::black(),
                bold: false,
                italic: false,
            });
        }
        self.text = text;
    }

    /// Appends a run covering `[start, end)` of the current text.
    ///
    /// Returns `false` (without modifying anything) if the range is
    /// out of bounds or overlaps an existing run.
    pub fn add_run(&mut self, run: TextRun) -> bool {
        if run.start >= run.end || run.end > self.text.len() {
            return false;
        }
        // Runs must be sorted, non-overlapping; reject overlaps.
        if self
            .runs
            .iter()
            .any(|r| run.start < r.end && r.start < run.end)
        {
            return false;
        }
        self.runs.push(run);
        self.runs.sort_by_key(|r| r.start);
        true
    }

    /// Removes the run covering byte `offset`, if any.  Returns its index
    /// into `runs` (or `None`).
    pub fn remove_run_at(&mut self, offset: usize) -> Option<usize> {
        let idx = self
            .runs
            .iter()
            .position(|r| r.start <= offset && offset < r.end)?;
        self.runs.remove(idx);
        Some(idx)
    }

    /// Index of the run containing byte `offset` (or `None`).
    pub fn run_at(&self, offset: usize) -> Option<usize> {
        self.runs
            .iter()
            .position(|r| r.start <= offset && offset < r.end)
    }

    /// Whether the runs are sorted, non-overlapping and within bounds.
    pub fn validate_runs(&self) -> bool {
        let mut prev_end = 0usize;
        for run in &self.runs {
            if run.start < prev_end || run.start >= run.end || run.end > self.text.len() {
                return false;
            }
            prev_end = run.end;
        }
        true
    }
}
