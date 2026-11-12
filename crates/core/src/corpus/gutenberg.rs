//! Project Gutenberg plain-text files wrap each book in a licence header and footer:
//!
//! ```text
//! The Project Gutenberg eBook of …            ← header (title, release date, licence)
//! *** START OF THE PROJECT GUTENBERG EBOOK … ***
//! …the book…
//! *** END OF THE PROJECT GUTENBERG EBOOK … ***
//! …footer (licence again)…
//! ```
//!
//! `body_range` finds the book. It refuses anything unexpected rather than guessing, because a
//! wrong range would make every citation into this edition wrong.

use std::fmt;
use std::ops::Range;

/// Both spellings Gutenberg has used over the years.
const START: [&[u8]; 2] = [
    b"*** START OF THE PROJECT GUTENBERG EBOOK",
    b"*** START OF THIS PROJECT GUTENBERG EBOOK",
];
const END: [&[u8]; 2] = [
    b"*** END OF THE PROJECT GUTENBERG EBOOK",
    b"*** END OF THIS PROJECT GUTENBERG EBOOK",
];

#[derive(Debug, PartialEq, Eq)]
pub enum GutenbergError {
    /// Expected exactly one START and one END marker line; found these counts.
    Markers {
        start: usize,
        end: usize,
    },
    EndBeforeStart,
    /// Nothing but whitespace between the markers.
    EmptyBody,
}

impl fmt::Display for GutenbergError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GutenbergError::Markers { start, end } => {
                write!(
                    f,
                    "expected one START and one END marker, found {start} and {end}"
                )
            }
            GutenbergError::EndBeforeStart => write!(f, "END marker comes before START marker"),
            GutenbergError::EmptyBody => write!(f, "no text between the markers"),
        }
    }
}

impl std::error::Error for GutenbergError {}

/// Byte range of the book: after the START line, before the END line, with surrounding
/// whitespace trimmed. Offsets index the original bytes (we never modify the file).
pub fn body_range(file: &[u8]) -> Result<Range<usize>, GutenbergError> {
    // Each line with the byte offset where it starts and where the next line starts.
    let mut starts = Vec::new(); // offset just after a START line
    let mut ends = Vec::new(); // offset where an END line begins
    let mut offset = 0;
    for line in file.split_inclusive(|&b| b == b'\n') {
        if START.iter().any(|m| line.starts_with(m)) {
            starts.push(offset + line.len());
        } else if END.iter().any(|m| line.starts_with(m)) {
            ends.push(offset);
        }
        offset += line.len();
    }

    let (&[body_start], &[body_end]) = (starts.as_slice(), ends.as_slice()) else {
        return Err(GutenbergError::Markers {
            start: starts.len(),
            end: ends.len(),
        });
    };
    if body_end < body_start {
        return Err(GutenbergError::EndBeforeStart);
    }

    let mut start = body_start + leading_whitespace(&file[body_start..body_end]);
    // Many files open the body with a volunteer credit ("Produced by …") — Gutenberg
    // metadata, not the work. Skip that first paragraph (up to the next blank line).
    if CREDITS.iter().any(|c| file[start..body_end].starts_with(c))
        && let Some(gap) = blank_line_after(&file[start..body_end])
    {
        start += gap;
        start += leading_whitespace(&file[start..body_end]);
    }
    let end = body_end
        - file[start..body_end]
            .iter()
            .rev()
            .take_while(|b| b.is_ascii_whitespace())
            .count();
    if start >= end {
        return Err(GutenbergError::EmptyBody);
    }
    Ok(start..end)
}

/// Openings of volunteer-credit paragraphs.
const CREDITS: [&[u8]; 3] = [b"Produced by", b"Transcribed by", b"E-text prepared by"];

fn leading_whitespace(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|b| b.is_ascii_whitespace()).count()
}

/// Offset just past the first blank line (`\n` followed by optional `\r` and `\n`), i.e. the
/// end of the first paragraph.
fn blank_line_after(bytes: &[u8]) -> Option<usize> {
    // Whichever comes first, so a file mixing LF and CRLF endings still splits correctly.
    let lf = bytes.windows(2).position(|w| w == b"\n\n").map(|i| i + 2);
    let crlf = bytes.windows(3).position(|w| w == b"\n\r\n").map(|i| i + 3);
    [lf, crlf].into_iter().flatten().min()
}
