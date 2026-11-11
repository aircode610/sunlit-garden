//! The Library compiler: raw file → structure.
//!
//! ```text
//! front-end (per format) → IR (this module) → passes → validation → store (or review queue)
//! ```
//!
//! The IR never copies or rewrites text: every unit and division is a **byte range into the
//! untouched original file**. Validation then proves the structure is complete and consistent
//! before anything is stored.

pub mod paragraphs;

use std::fmt;
use std::ops::Range;

/// What a unit of text is. Every non-whitespace byte of the body is in exactly one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    /// The author's words: becomes a passage.
    Text,
    /// A division's own heading line ("PART 1", "INTRODUCTION").
    DivisionHeading,
    /// A heading an editor added to a section ("HOW FAR THE ASSENT OF THE VULGAR CONCEDED.--").
    EditorHeading,
    /// An editor's note ("[Note: …]").
    EditorNote,
    /// Deliberately not part of the work's text, with the reason ("table of contents").
    Skipped(&'static str),
}

/// A unit: what it is and exactly which bytes it covers (trimmed: no surrounding whitespace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub kind: UnitKind,
    pub range: Range<usize>,
}

/// A node of the edition's own structure ("Part 1", "Introduction").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivisionNode {
    /// `part`, `introduction`, `preface`, `letter`, …
    pub kind: String,
    /// As printed: "1", "Introduction".
    pub label: String,
    /// The locator segment for passages inside it: `part-1`, `introduction`.
    pub slug: String,
    pub range: Range<usize>,
    pub children: Vec<DivisionNode>,
}

/// The compiled structure of one edition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    /// The work's text inside the file (e.g. between Gutenberg's markers).
    pub body: Range<usize>,
    pub divisions: Vec<DivisionNode>,
    /// In file order.
    pub units: Vec<Unit>,
}

/// A reason the compiled structure can't be trusted. Any problem sends the edition to review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Non-whitespace text that no unit covers (shown with its range and a snippet).
    Uncovered { range: Range<usize>, text: String },
    /// Two units claim the same bytes.
    Overlap { first: Range<usize>, second: Range<usize> },
    /// A unit lies (partly) outside the body.
    OutsideBody { range: Range<usize> },
    /// A unit starts or ends with whitespace, or is empty.
    Untrimmed { range: Range<usize> },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Uncovered { range, text } => write!(f, "bytes {range:?} are in no unit: {text:?}"),
            Problem::Overlap { first, second } => write!(f, "units {first:?} and {second:?} overlap"),
            Problem::OutsideBody { range } => write!(f, "unit {range:?} lies outside the body"),
            Problem::Untrimmed { range } => write!(f, "unit {range:?} is empty or has surrounding whitespace"),
        }
    }
}

/// Coverage: units are trimmed, inside the body, don't overlap, and together cover every
/// non-whitespace byte of the body. Returns every problem found (empty = pass).
pub fn check_coverage(file: &[u8], compiled: &Compiled) -> Vec<Problem> {
    let mut problems = Vec::new();
    let body = compiled.body.clone();

    let mut units: Vec<&Unit> = compiled.units.iter().collect();
    units.sort_by_key(|u| (u.range.start, u.range.end));

    for unit in &units {
        let r = unit.range.clone();
        if r.start < body.start || r.end > body.end {
            problems.push(Problem::OutsideBody { range: r.clone() });
        }
        let bytes = &file[r.start.min(file.len())..r.end.min(file.len())];
        let trimmed = !bytes.is_empty()
            && !bytes.first().is_some_and(u8::is_ascii_whitespace)
            && !bytes.last().is_some_and(u8::is_ascii_whitespace);
        if !trimmed {
            problems.push(Problem::Untrimmed { range: r });
        }
    }

    // Walk the body left to right; `cursor` = end of everything covered so far.
    let mut cursor = body.start;
    for pair in units.windows(2) {
        if pair[1].range.start < pair[0].range.end {
            problems.push(Problem::Overlap { first: pair[0].range.clone(), second: pair[1].range.clone() });
        }
    }
    for unit in &units {
        let start = unit.range.start.max(body.start);
        if start > cursor {
            report_gap(file, cursor..start, &mut problems);
        }
        cursor = cursor.max(unit.range.end.min(body.end));
    }
    if cursor < body.end {
        report_gap(file, cursor..body.end, &mut problems);
    }
    problems
}

/// A gap between units is fine if it's only whitespace; otherwise that text is uncovered.
fn report_gap(file: &[u8], gap: Range<usize>, problems: &mut Vec<Problem>) {
    let bytes = &file[gap.clone()];
    if bytes.iter().any(|b| !b.is_ascii_whitespace()) {
        let snippet: String = String::from_utf8_lossy(bytes).trim().chars().take(80).collect();
        problems.push(Problem::Uncovered { range: gap, text: snippet });
    }
}
