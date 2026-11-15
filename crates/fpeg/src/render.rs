// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Errors shown with the line of the text they stand in, the way rustc shows its errors.

use std::fmt::Write as _;

use crate::lines::LineIndex;
use crate::parse::{ParseError, ParseErrorKind, Recovery};

/// The columns from one tab stop to the next in a shown line.
const TAB_COLUMNS: usize = 4;

/// The widest line shown. A wider one is cut around the error.
const MAX_LINE_COLUMNS: usize = 120;

/// The columns that a line cut on its left keeps before the error.
const CONTEXT_COLUMNS: usize = 40;

/// What stands for the text left out where a line is cut.
const CUT: &str = "...";

/// Where Unicode's control pictures start, the symbol for the control character 0.
const CONTROL_PICTURES: u32 = 0x2400;

/// Unicode's symbol for the control character DEL.
const DELETE_PICTURE: char = '\u{2421}';

/// A text with its lines indexed and the path it was read from, to show errors in.
#[derive(Clone, Debug)]
pub struct Source<'t> {
    text: &'t str,
    path: Option<&'t str>,
    lines: LineIndex,
}

impl<'t> Source<'t> {
    /// Index the lines of `text`, which errors name by `path`, or by line and column alone.
    pub fn new(text: &'t str, path: Option<&'t str>) -> Source<'t> {
        Source {
            text,
            path,
            lines: LineIndex::new(text),
        }
    }

    /// The byte offset before the code point at index `chars`, for callers that count in code
    /// points.
    pub fn byte_offset(&self, chars: usize) -> usize {
        self.lines.byte_offset(self.text, chars)
    }

    /// An error shown as rustc shows one: `error[name]: message` for a named error and
    /// `error: message` for others, then the path, line and column of `start`, and the line
    /// that holds it with carets under the span up to `end` or the end of the line. `start` and
    /// `end` are byte offsets. A message that starts with the name, as a named error's does,
    /// says it once.
    ///
    /// Tabs show as spaces, and control and bidirectional characters as visible marks, so that
    /// no text can rewrite the report on a terminal. A line too wide is cut around the error.
    pub fn render(&self, name: Option<&str>, message: &str, start: usize, end: usize) -> String {
        let start = self.text.floor_char_boundary(start);
        let end = self.text.floor_char_boundary(end).max(start);
        let line = self.lines.line(start);
        let (from, to) = self.lines.bounds(self.text, line);
        let text = self.text.get(from..to).unwrap_or_default();
        let first = start.min(to).saturating_sub(from);
        let shown = Shown::new(text, first, end.min(to).saturating_sub(from));
        let column = self.lines.position(self.text, start).column;
        let number = (line + 1).to_string();
        let pad = " ".repeat(number.len());
        let mut out = error_title(name, message);
        let _ = write!(out, "\n{pad}--> ");
        if let Some(path) = self.path {
            push_shown(&mut out, path);
            out.push(':');
        }
        let _ = write!(out, "{number}:{column}\n{pad} |\n{number} |");
        if !shown.text.is_empty() {
            out.push(' ');
            out.push_str(&shown.text);
        }
        let carets = "^".repeat(shown.end.saturating_sub(shown.start).max(1));
        let _ = write!(out, "\n{pad} | {}{carets}", " ".repeat(shown.start));
        out
    }
}

impl ParseError {
    /// Every error of the parse as [`Source::render`] shows it, in the order of the text: the
    /// recoveries on the path to this error, then this error. A start rule that the grammar
    /// lacks has no place in the text, so its error shows the title alone.
    pub fn render(&self, source: &Source<'_>) -> String {
        let mut shown: Vec<String> = (self.recoveries.iter())
            .map(|recovery| recovery.render(source))
            .collect();
        let name = match &self.kind {
            ParseErrorKind::UnknownStart(_) => {
                shown.push(error_title(None, &self.message()));
                return shown.join("\n\n");
            }
            ParseErrorKind::Label {
                label, named: true, ..
            } => Some(label.as_str()),
            _ => None,
        };
        shown.push(source.render(name, &self.message(), self.position, self.end()));
        shown.join("\n\n")
    }
}

impl Recovery {
    /// The error as [`Source::render`] shows it, where the parse got furthest, with carets to
    /// the end of what the recovery took.
    pub fn render(&self, source: &Source<'_>) -> String {
        let end = self.end.max(self.position);
        source.render(Some(&self.label), &self.message(), self.position, end)
    }
}

/// The first line of an error as [`Source::render`] shows it, all that an error without a
/// place in a text shows.
pub fn error_title(name: Option<&str>, message: &str) -> String {
    let mut out = String::from("error");
    let rest = match name {
        Some(name) => {
            out.push('[');
            push_shown(&mut out, name);
            out.push(']');
            match message.strip_prefix(name) {
                Some(rest) if rest.is_empty() => rest,
                Some(rest) => rest.strip_prefix(": ").unwrap_or(message),
                None => message,
            }
        }
        None => message,
    };
    if !rest.is_empty() {
        out.push_str(": ");
        push_shown(&mut out, rest);
    }
    out
}

/// A line as shown, and the columns before a span on it and before its end.
struct Shown {
    text: String,
    start: usize,
    end: usize,
}

impl Shown {
    /// Show `line` with the span from byte `first` to byte `last`. A line too wide is cut: on
    /// the left where the span starts too far right to show with context after it, keeping
    /// [`CONTEXT_COLUMNS`] before it, and on the right where it goes on past
    /// [`MAX_LINE_COLUMNS`]. The work is bounded by what is shown, however long the line.
    fn new(line: &str, first: usize, last: usize) -> Shown {
        let before = line.get(..first).unwrap_or(line);
        let mut width = 0;
        let far = before.chars().any(|c| {
            width += reach(c);
            width > MAX_LINE_COLUMNS - CONTEXT_COLUMNS
        });
        let mut from = 0;
        if far {
            from = first;
            let mut width = 0;
            for (i, c) in before.char_indices().rev() {
                width += reach(c);
                if width > CONTEXT_COLUMNS {
                    break;
                }
                from = i;
            }
        }
        let mut text = String::new();
        let mut column = 0;
        if from > 0 {
            text.push_str(CUT);
            column = CUT.len();
        }
        let (mut start, mut end) = (None, None);
        let mut cut = false;
        // The columns shown so far with at least one for each character, as `reach` counts.
        let mut width = column;
        for (i, c) in line.get(from..).unwrap_or_default().char_indices() {
            let i = from + i;
            if i >= first && start.is_none() {
                start = Some(column);
            }
            if i >= last && end.is_none() {
                end = Some(column);
            }
            if width >= MAX_LINE_COLUMNS && i > first {
                cut = true;
                break;
            }
            let taken = push_char(&mut text, c, column);
            column += taken;
            width += taken.max(1);
        }
        let (start, end) = (start.unwrap_or(column), end.unwrap_or(column));
        if cut {
            text.push_str(CUT);
        }
        Shown { text, start, end }
    }
}

/// Append `text` with each character as a shown line holds it.
fn push_shown(out: &mut String, text: &str) {
    let mut column = 0;
    for c in text.chars() {
        column += push_char(out, c, column);
    }
}

/// Append a character as a shown line holds it, and return the columns it takes at `column`:
/// a tab as spaces to the next tab stop, a control character as its control picture, and a C1
/// control, a bidirectional control or a line or paragraph separator as the replacement
/// character, since terminals act on them.
fn push_char(out: &mut String, c: char, column: usize) -> usize {
    match c {
        '\t' => {
            let spaces = TAB_COLUMNS - column % TAB_COLUMNS;
            out.extend(std::iter::repeat_n(' ', spaces));
            spaces
        }
        '\0'..='\x1f' => {
            let picture = char::from_u32(CONTROL_PICTURES + u32::from(c));
            out.push(picture.unwrap_or(char::REPLACEMENT_CHARACTER));
            1
        }
        '\x7f' => {
            out.push(DELETE_PICTURE);
            1
        }
        '\u{80}'..='\u{9f}'
        | '\u{61c}'
        | '\u{200e}'
        | '\u{200f}'
        | '\u{2028}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}' => {
            out.push(char::REPLACEMENT_CHARACTER);
            1
        }
        c => {
            out.push(c);
            columns(c)
        }
    }
}

/// The most columns a character can take in a shown line, where those of a tab depend on its
/// place, and at least one, so that a run of characters that take none is cut as well.
fn reach(c: char) -> usize {
    match c {
        '\t' => TAB_COLUMNS,
        c => columns(c).max(1),
    }
}

/// The columns a terminal gives a character: none for combining marks and joiners, two for the
/// wide characters of East Asian scripts and for emoji, one for the rest. The main ranges of
/// Unicode's East Asian Width, not all of them.
fn columns(c: char) -> usize {
    match u32::from(c) {
        0x0300..=0x036F
        | 0x0483..=0x0489
        | 0x0591..=0x05BD
        | 0x0610..=0x061A
        | 0x064B..=0x065F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x200B..=0x200D
        | 0x2060..=0x2064
        | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F
        | 0xFEFF
        | 0xE0100..=0xE01EF => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE10..=0xFE19
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F680..=0x1F6FF
        | 0x1F900..=0x1F9FF
        | 0x1FA70..=0x1FAFF
        | 0x20000..=0x2FFFD
        | 0x30000..=0x3FFFD => 2,
        _ => 1,
    }
}
