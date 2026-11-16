// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lines and columns of a text, and code point offsets for callers that count in code points.

/// Bytes from one checkpoint of a text beyond ASCII to the next. Converting a byte offset to
/// code points counts from the checkpoint before it, so this bounds the work of one conversion.
const CHECKPOINT_BYTES: usize = 4096;

/// The start offsets of the lines of a text. Line breaks are `\r\n`, `\n` and `\r`.
#[derive(Clone, Debug)]
pub struct LineIndex {
    starts: Vec<usize>,
    /// Whether the text is ASCII, where a code point is a byte.
    ascii: bool,
    /// For a text beyond ASCII, the code points before every multiple of
    /// [`CHECKPOINT_BYTES`].
    checkpoints: Vec<usize>,
}

/// A place in a text as people count it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// The code points before it.
    pub offset: usize,
    /// The line, counting from 1.
    pub line: usize,
    /// The column, counting from 1, in code points from the start of the line.
    pub column: usize,
}

/// The code points in a range of bytes: the bytes that do not continue a UTF-8 sequence. A
/// range that starts or ends inside a code point counts the code point once.
fn count_chars(text: &str, from: usize, to: usize) -> usize {
    let bytes = text.as_bytes();
    let to = to.min(bytes.len());
    let from = from.min(to);
    bytes[from..to]
        .iter()
        .filter(|&&b| b & 0xC0 != 0x80)
        .count()
}

impl LineIndex {
    /// Index the lines of `text`.
    pub fn new(text: &str) -> LineIndex {
        let bytes = text.as_bytes();
        let mut starts = vec![0];
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => starts.push(i + 1),
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                    starts.push(i + 2);
                    i += 1;
                }
                b'\r' => starts.push(i + 1),
                _ => {}
            }
            i += 1;
        }
        let ascii = text.is_ascii();
        let mut checkpoints = Vec::new();
        if !ascii {
            let mut chars = 0;
            for at in (0..=bytes.len()).step_by(CHECKPOINT_BYTES) {
                if at > 0 {
                    chars += count_chars(text, at - CHECKPOINT_BYTES, at);
                }
                checkpoints.push(chars);
            }
        }
        LineIndex {
            starts,
            ascii,
            checkpoints,
        }
    }

    /// The line of a byte offset, counting from 0.
    pub fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|&start| start <= offset) - 1
    }

    /// The start offset of the line that holds `offset`.
    pub fn line_start(&self, offset: usize) -> usize {
        self.starts[self.line(offset)]
    }

    /// The byte range of a line counted from 0, without its line break. `text` must be the
    /// text the index was made for.
    pub(crate) fn bounds(&self, text: &str, line: usize) -> (usize, usize) {
        let start = self.starts.get(line).copied().unwrap_or(text.len());
        let next = self.starts.get(line + 1).copied().unwrap_or(text.len());
        let raw = text.get(start..next).unwrap_or_default();
        let content = raw.strip_suffix('\n').unwrap_or(raw);
        let content = content.strip_suffix('\r').unwrap_or(content);
        (start, start + content.len())
    }

    /// The code points of `text` before a byte offset, the index of a string that counts in
    /// code points. `text` must be the text the index was made for.
    pub fn char_offset(&self, text: &str, offset: usize) -> usize {
        let offset = offset.min(text.len());
        if self.ascii {
            return offset;
        }
        let checkpoint = offset / CHECKPOINT_BYTES;
        let before = self.checkpoints.get(checkpoint).copied().unwrap_or(0);
        before + count_chars(text, checkpoint * CHECKPOINT_BYTES, offset)
    }

    /// The byte offset before the code point at index `chars`, the inverse of
    /// [`LineIndex::char_offset`], and the end of the text for an index past its code points.
    /// `text` must be the text the index was made for.
    pub fn byte_offset(&self, text: &str, chars: usize) -> usize {
        if self.ascii {
            return chars.min(text.len());
        }
        let checkpoint =
            (self.checkpoints.partition_point(|&before| before <= chars)).saturating_sub(1);
        let mut count = self.checkpoints.get(checkpoint).copied().unwrap_or(0);
        let bytes = text.as_bytes();
        let mut at = checkpoint * CHECKPOINT_BYTES;
        loop {
            // A checkpoint may fall inside a code point, whose rest counts with it.
            while bytes.get(at).is_some_and(|&b| b & 0xC0 == 0x80) {
                at += 1;
            }
            if count == chars || at >= bytes.len() {
                return at.min(bytes.len());
            }
            count += 1;
            at += 1;
        }
    }

    /// The position of a byte offset of `text`.
    pub fn position(&self, text: &str, offset: usize) -> Position {
        let line = self.line(offset);
        let chars = self.char_offset(text, offset);
        let line_chars = self.char_offset(text, self.starts[line]);
        Position {
            offset: chars,
            line: line + 1,
            column: chars - line_chars + 1,
        }
    }

    /// The line and column of a byte offset as people count them: both from 1, the column in
    /// code points from the start of the line, whatever the characters are.
    pub fn line_column(&self, text: &str, offset: usize) -> (usize, usize) {
        let position = self.position(text, offset);
        (position.line, position.column)
    }

    /// The column of a byte offset, counting from 0: one per code point from the start of the
    /// line, with a tab advancing to the next multiple of `tab_width` and a form feed going back
    /// to 0, as Python counts indentation. The reference interpreter's, as the machine counts
    /// its own.
    #[cfg(feature = "reference")]
    pub(crate) fn column(&self, text: &str, offset: usize, tab_width: u32) -> u32 {
        let start = self.line_start(offset);
        let mut column = 0u32;
        // Saturating, so that a line too long to count stays at the largest column.
        for c in text.get(start..offset).unwrap_or("").chars() {
            column = match c {
                '\t' if tab_width > 0 => (column / tab_width + 1).saturating_mul(tab_width),
                '\x0C' => 0,
                _ => column.saturating_add(1),
            };
        }
        column
    }
}

/// Positions of byte offsets that a scan meets in increasing order, each counted on from the one
/// before, so that a scan over a whole text counts each byte once however long its lines are.
#[derive(Clone, Debug)]
pub struct LineCursor<'t> {
    lines: &'t LineIndex,
    text: &'t str,
    /// The line of the last position, from 0.
    line: usize,
    /// The byte offset and code point offset of the last position.
    byte: usize,
    chars: usize,
    /// The code point offset where the line of the last position starts.
    line_chars: usize,
}

impl<'t> LineCursor<'t> {
    /// A cursor at the start of `text`, which `lines` indexes.
    pub fn new(lines: &'t LineIndex, text: &'t str) -> LineCursor<'t> {
        LineCursor {
            lines,
            text,
            line: 0,
            byte: 0,
            chars: 0,
            line_chars: 0,
        }
    }

    fn count(&self, from: usize, to: usize) -> usize {
        if self.lines.ascii {
            to - from
        } else {
            count_chars(self.text, from, to)
        }
    }

    /// The position of a byte offset. An offset before the last one is found as
    /// [`LineIndex::position`] finds it, and the cursor moves back to it.
    pub fn position(&mut self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        if offset < self.byte {
            let position = self.lines.position(self.text, offset);
            self.line = position.line - 1;
            self.byte = offset;
            self.chars = position.offset;
            self.line_chars = position.offset + 1 - position.column;
            return position;
        }
        let starts = &self.lines.starts;
        while let Some(&next) = starts.get(self.line + 1)
            && next <= offset
        {
            self.chars += self.count(self.byte, next);
            self.byte = next;
            self.line += 1;
            self.line_chars = self.chars;
        }
        self.chars += self.count(self.byte, offset);
        self.byte = offset;
        Position {
            offset: self.chars,
            line: self.line + 1,
            column: self.chars - self.line_chars + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CHECKPOINT_BYTES, LineCursor, LineIndex, Position};

    #[test]
    fn columns_count_code_points_expand_tabs_and_restart_at_form_feeds() {
        let text = "ab\r\n\täx\rcd\n  \x0C y";
        let lines = LineIndex::new(text);
        let x = text.find('x').unwrap_or_default();
        assert_eq!(lines.line(x), 1);
        assert_eq!(lines.column(text, x, 8), 9);
        assert_eq!(lines.column(text, x, 4), 5);
        assert_eq!(lines.line(text.find('d').unwrap_or_default()), 2);
        assert_eq!(lines.column(text, text.len() - 1, 8), 1);
    }

    #[test]
    fn people_count_lines_and_columns_from_one_in_code_points() {
        let text = "ab\r\n\täx";
        let lines = LineIndex::new(text);
        assert_eq!(lines.line_column(text, 0), (1, 1));
        assert_eq!(lines.line_column(text, text.len() - 1), (2, 3));
        assert_eq!(lines.line_column(text, text.len()), (2, 4));
    }

    #[test]
    fn a_column_too_large_to_count_stays_at_the_largest() {
        let text = "\t\t\tx";
        let lines = LineIndex::new(text);
        assert_eq!(lines.column(text, 3, u32::MAX / 2 + 1), u32::MAX);
    }

    /// The position of every code point boundary, counted the slow way.
    fn counted(text: &str) -> Vec<(usize, Position)> {
        let mut out = Vec::new();
        let (mut line, mut column, mut offset) = (1, 1, 0);
        let mut chars = text.char_indices().peekable();
        loop {
            let Some(&(at, c)) = chars.peek() else {
                out.push((
                    text.len(),
                    Position {
                        offset,
                        line,
                        column,
                    },
                ));
                return out;
            };
            out.push((
                at,
                Position {
                    offset,
                    line,
                    column,
                },
            ));
            chars.next();
            offset += 1;
            let breaks = c == '\n' || (c == '\r' && chars.peek().map(|&(_, n)| n) != Some('\n'));
            if breaks {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
    }

    #[test]
    fn positions_count_code_points_across_checkpoints_and_long_lines() {
        // Lines of mixed widths, then one line three checkpoints long, three bytes a step.
        let mut text = String::new();
        for i in 0..300 {
            text.push_str(["ab\n", "\u{e4}\u{20ac}x\r\n", "\u{1f600}\r", "\t\u{3b1}\n"][i % 4]);
        }
        for _ in 0..CHECKPOINT_BYTES {
            text.push_str("a\u{e9}");
        }
        let lines = LineIndex::new(&text);
        let mut cursor = LineCursor::new(&lines, &text);
        for (k, (at, expected)) in counted(&text).into_iter().enumerate() {
            assert_eq!(cursor.position(at), expected, "the cursor at byte {at}");
            // Each lookup counts up to a checkpoint of bytes, so a sample keeps the test fast.
            if k % 13 == 0 {
                assert_eq!(lines.position(&text, at), expected, "at byte {at}");
                assert_eq!(lines.byte_offset(&text, expected.offset), at);
            }
        }
        // A cursor that goes back finds the place and counts on from there.
        let (back, expected) = counted(&text)[5];
        assert_eq!(cursor.position(back), expected);
        let (on, expected) = counted(&text)[9];
        assert_eq!(cursor.position(on), expected);
    }

    #[test]
    fn an_ascii_text_counts_code_points_as_bytes() {
        let text = "one\ntwo\r\nthree";
        let lines = LineIndex::new(text);
        let mut cursor = LineCursor::new(&lines, text);
        for (at, expected) in counted(text) {
            assert_eq!(lines.position(text, at), expected);
            assert_eq!(cursor.position(at), expected);
            assert_eq!(lines.byte_offset(text, expected.offset), at);
        }
        assert_eq!(lines.byte_offset(text, text.len() + 1), text.len());
    }
}
