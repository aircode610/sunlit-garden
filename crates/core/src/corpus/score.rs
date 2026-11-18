//! The compiler scorer: how well does our plain-text compiler find a book's structure?
//!
//! Both editions are reduced to the same shape, a [`Segmented`] text: the book's words
//! (normalised, so spelling of punctuation and case don't matter), where each passage starts and
//! where each division (Section, Part…) starts. The answer key comes from a hand-structured
//! edition (Standard Ebooks); the compiled edition is aligned to it word by word, and its
//! boundaries are scored against the answer key's.

use super::xhtml::{Token, XhtmlError, tokenize};

/// A text as words, with its passage and division boundaries as word positions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Segmented {
    /// Normalised words in reading order (see [`words`]).
    pub words: Vec<String>,
    /// Word index where each passage starts, ascending.
    pub boundaries: Vec<usize>,
    /// Divisions in reading order.
    pub divisions: Vec<DivisionMark>,
}

/// Where a division starts, how deeply it's nested (1 = top level) and its label (a number for
/// numbered divisions — "X" and "10" both become `10` — otherwise the normalised title).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivisionMark {
    pub at: usize,
    pub depth: usize,
    pub label: String,
}

/// Normalised words: lowercase, letters and digits only ("Tillotson’s" → `tillotsons`).
pub fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
}

/// A division label as compared by the scorer: roman or arabic numbers become arabic ("XII." →
/// `12`), anything else its normalised words.
pub fn label(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches('.');
    if let Ok(n) = trimmed.parse::<u32>() {
        return n.to_string();
    }
    if let Some(n) = roman(trimmed) {
        return n.to_string();
    }
    words(text).collect::<Vec<_>>().join(" ")
}

fn roman(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let value = |c: char| match c.to_ascii_uppercase() {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        'C' => Some(100),
        'D' => Some(500),
        'M' => Some(1000),
        _ => None,
    };
    let digits: Vec<u32> = s.chars().map(value).collect::<Option<_>>()?;
    let mut total = 0;
    for (i, &d) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|&next| next > d) {
            total -= d as i64;
        } else {
            total += d as i64;
        }
    }
    u32::try_from(total).ok().filter(|&n| n > 0)
}

/// The answer key from a Standard Ebooks book: its text files in reading order (`content.opf`
/// spine), as (file name, XHTML). Sections typed chapter / subchapter / part / division open
/// divisions, labelled by their heading's ordinal; every body `<p>` starts a passage. Heading text
/// and note-reference numbers are not part of the book's words.
pub fn standard_ebooks_gold(spine: &[(String, String)]) -> Result<Segmented, XhtmlError> {
    let mut out = Segmented::default();
    for (_, xhtml) in spine {
        let mut depth = 0;
        // Open elements: (name, epub:type, is a division section).
        let mut stack: Vec<(String, String, bool)> = Vec::new();
        let mut heading: Option<(usize, String, String)> = None; // (depth, ordinal, all text)
        let mut in_p = 0;
        let mut p_start: Option<usize> = None;
        for token in tokenize(xhtml)? {
            match token {
                Token::Start { name, attrs } => {
                    let ty = attrs
                        .iter()
                        .find(|(k, _)| k == "epub:type")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default();
                    let is_division = name == "section"
                        && ty.split_whitespace().any(|t| {
                            matches!(
                                t,
                                "chapter"
                                    | "z3998:subchapter"
                                    | "part"
                                    | "division"
                                    | "z3998:section"
                            )
                        });
                    if is_division {
                        depth += 1;
                    }
                    let is_heading = name == "hgroup"
                        || (name.len() == 2
                            && name.starts_with('h')
                            && name[1..].parse::<u8>().is_ok());
                    if is_heading && heading.is_none() {
                        heading = Some((depth, String::new(), String::new()));
                    }
                    if name == "p" && heading.is_none() {
                        in_p += 1;
                        if in_p == 1 {
                            p_start = Some(out.words.len());
                        }
                    }
                    stack.push((name, ty, is_division));
                }
                Token::End { name } => {
                    let (_, _, was_division) = stack.pop().expect("tokenize guarantees nesting");
                    if was_division {
                        depth -= 1;
                    }
                    let is_heading = name == "hgroup"
                        || (name.len() == 2
                            && name.starts_with('h')
                            && name[1..].parse::<u8>().is_ok());
                    let heading_done = is_heading && !stack.iter().any(|(n, _, _)| n == "hgroup");
                    if heading_done && let Some((d, ordinal, all)) = heading.take() {
                        let text = if ordinal.trim().is_empty() {
                            all
                        } else {
                            ordinal
                        };
                        out.divisions.push(DivisionMark {
                            at: out.words.len(),
                            depth: d,
                            label: label(&text),
                        });
                    }
                    if name == "p" && in_p > 0 {
                        in_p -= 1;
                        if in_p == 0 {
                            if let Some(start) = p_start.take().filter(|&s| s < out.words.len()) {
                                out.boundaries.push(start);
                            }
                        }
                    }
                }
                Token::Text(text) => {
                    let in_noteref = stack
                        .iter()
                        .any(|(_, ty, _)| ty.split_whitespace().any(|t| t == "noteref"));
                    if let Some((_, ordinal, all)) = heading.as_mut() {
                        let is_ordinal = stack
                            .last()
                            .is_some_and(|(_, ty, _)| ty.contains("ordinal"));
                        if is_ordinal {
                            ordinal.push_str(&text);
                        }
                        all.push_str(&text);
                        all.push(' ');
                    } else if in_p > 0 && !in_noteref {
                        out.words.extend(words(&text));
                    }
                }
            }
        }
    }
    Ok(out)
}
