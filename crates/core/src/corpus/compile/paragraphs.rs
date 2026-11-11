//! Paragraphs: blocks of text separated by blank lines. The fallback every plain text gets, so
//! any book is importable and citable even before its structure is recognised.

use std::ops::Range;

/// The trimmed byte ranges of every paragraph in `body`. A blank line is a line containing
/// only whitespace (handles `\n` and `\r\n` endings).
pub fn paragraphs(file: &[u8], body: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    // Start of the paragraph being collected, and end of its last non-blank line.
    let mut current: Option<(usize, usize)> = None;
    let mut offset = body.start;

    for line in file[body.clone()].split_inclusive(|&b| b == b'\n') {
        let line_range = offset..offset + line.len();
        offset += line.len();
        match trimmed(file, line_range) {
            // A non-blank line: start a paragraph or extend the current one.
            Some(text) => {
                current = Some(match current {
                    Some((start, _)) => (start, text.end),
                    None => (text.start, text.end),
                });
            }
            // A blank line ends the current paragraph, if any.
            None => {
                if let Some((start, end)) = current.take() {
                    out.push(start..end);
                }
            }
        }
    }
    if let Some((start, end)) = current {
        out.push(start..end);
    }
    out
}

/// `range` without leading/trailing whitespace, or None if it's all whitespace.
pub fn trimmed(file: &[u8], range: Range<usize>) -> Option<Range<usize>> {
    let bytes = &file[range.clone()];
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace())?;
    let end = bytes.iter().rposition(|b| !b.is_ascii_whitespace())? + 1;
    Some(range.start + start..range.start + end)
}
