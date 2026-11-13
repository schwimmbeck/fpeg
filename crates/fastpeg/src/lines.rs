// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lines and columns of a text.

/// The start offsets of the lines of a text. Line breaks are `\r\n`, `\n` and `\r`.
#[derive(Clone, Debug)]
pub struct LineIndex {
    starts: Vec<usize>,
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
        LineIndex { starts }
    }

    /// The line of a byte offset, counting from 0.
    pub fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|&start| start <= offset) - 1
    }

    /// The start offset of the line that holds `offset`.
    pub fn line_start(&self, offset: usize) -> usize {
        self.starts[self.line(offset)]
    }

    /// The column of a byte offset, counting from 0: one per code point from the start of the
    /// line, with a tab advancing to the next multiple of `tab_width` and a form feed going back
    /// to 0, as Python counts indentation.
    pub fn column(&self, text: &str, offset: usize, tab_width: u32) -> u32 {
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

#[cfg(test)]
mod tests {
    use super::LineIndex;

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
    fn a_column_too_large_to_count_stays_at_the_largest() {
        let text = "\t\t\tx";
        let lines = LineIndex::new(text);
        assert_eq!(lines.column(text, 3, u32::MAX / 2 + 1), u32::MAX);
    }
}
