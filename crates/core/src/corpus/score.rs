//! The compiler scorer: how well does our plain-text compiler find a book's structure?
//!
//! Both editions are reduced to the same shape, a [`Segmented`] text: the book's words
//! (normalised, so spelling of punctuation and case don't matter), where each passage starts and
//! where each division (Section, Part…) starts. The answer key comes from a hand-structured
//! edition (Standard Ebooks); the compiled edition is aligned to it word by word, and its
//! boundaries are scored against the answer key's.

use super::compile::DivisionNode;
use super::compile::structure::{PlannedRole, Structure};
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
                        if in_p == 0
                            && let Some(start) = p_start.take().filter(|&s| s < out.words.len())
                        {
                            out.boundaries.push(start);
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

/// Our compiler's view of an edition in the same shape: the words of every author-text passage
/// in order, a boundary where each (non-empty) passage starts, and a mark at the first passage
/// inside each division, its depth taken from the division tree.
pub fn segment(file: &[u8], structure: &Structure) -> Segmented {
    let mut out = Segmented::default();
    // Byte position of each passage's first word, to place divisions.
    let mut starts: Vec<(usize, usize)> = Vec::new(); // (byte start, word index)
    for (i, p) in structure.passages.iter().enumerate() {
        if p.role != PlannedRole::Text {
            continue;
        }
        let at = out.words.len();
        out.words.extend(words(&structure.author_words(file, i)));
        if out.words.len() > at {
            out.boundaries.push(at);
            starts.push((p.range.start, at));
        }
    }
    fn walk(
        nodes: &[DivisionNode],
        depth: usize,
        starts: &[(usize, usize)],
        end: usize,
        out: &mut Vec<DivisionMark>,
    ) {
        for node in nodes {
            // First passage that starts inside the division; an empty division marks where the
            // next passage begins.
            let at = starts
                .iter()
                .find(|(b, _)| *b >= node.range.start)
                .map_or(end, |&(_, w)| w);
            out.push(DivisionMark {
                at,
                depth,
                label: label(&node.label),
            });
            walk(&node.children, depth + 1, starts, end, out);
        }
    }
    let mut divisions = Vec::new();
    walk(
        &structure.compiled.divisions,
        1,
        &starts,
        out.words.len(),
        &mut divisions,
    );
    out.divisions = divisions;
    out
}

/// Gaps up to this many cells (len a × len b) get an exact longest-common-subsequence pass.
const LCS_CELLS: usize = 4_000_000;

/// Pair equal words of `a` and `b`, in order (both indices strictly increase): patience diff.
/// Words that occur exactly once in both are anchors (the longest run of them in order); the
/// gaps between anchors are aligned the same way, and small gaps exactly (LCS). Large stretches
/// on one side only (an index, a translator's notes) simply stay unpaired.
pub fn align(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    // Ranges still to align: (a_start, a_end, b_start, b_end).
    let mut todo = vec![(0, a.len(), 0, b.len())];
    while let Some((mut a0, mut a1, mut b0, mut b1)) = todo.pop() {
        // Common prefix and suffix match directly.
        while a0 < a1 && b0 < b1 && a[a0] == b[b0] {
            pairs.push((a0, b0));
            a0 += 1;
            b0 += 1;
        }
        while a0 < a1 && b0 < b1 && a[a1 - 1] == b[b1 - 1] {
            a1 -= 1;
            b1 -= 1;
            pairs.push((a1, b1));
        }
        if a0 == a1 || b0 == b1 {
            continue;
        }
        let anchors = unique_anchors(&a[a0..a1], &b[b0..b1]);
        if anchors.is_empty() {
            if (a1 - a0) * (b1 - b0) <= LCS_CELLS {
                pairs.extend(
                    lcs(&a[a0..a1], &b[b0..b1])
                        .into_iter()
                        .map(|(i, j)| (a0 + i, b0 + j)),
                );
            }
            continue;
        }
        // Align between consecutive anchors (and before the first / after the last).
        let (mut pa, mut pb) = (a0, b0);
        for (i, j) in anchors {
            let (i, j) = (a0 + i, b0 + j);
            todo.push((pa, i, pb, j));
            pairs.push((i, j));
            pa = i + 1;
            pb = j + 1;
        }
        todo.push((pa, a1, pb, b1));
    }
    pairs.sort_unstable();
    pairs
}

/// Words occurring exactly once in `a` and once in `b`, paired, reduced to the longest run that
/// increases in both (longest increasing subsequence by `b` position, patience sorting).
fn unique_anchors(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    use std::collections::HashMap;
    let mut count: HashMap<&str, (usize, usize, usize)> = HashMap::new(); // (in a, in b, a pos)
    for (i, w) in a.iter().enumerate() {
        let e = count.entry(w).or_default();
        e.0 += 1;
        e.2 = i;
    }
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    let mut in_b: HashMap<&str, (usize, usize)> = HashMap::new(); // (count, b pos)
    for (j, w) in b.iter().enumerate() {
        let e = in_b.entry(w).or_default();
        e.0 += 1;
        e.1 = j;
    }
    for (w, (n_b, j)) in &in_b {
        if *n_b == 1
            && let Some(&(1, _, i)) = count.get(w)
        {
            candidates.push((i, *j));
        }
    }
    candidates.sort_unstable();
    // Longest increasing subsequence of the b positions.
    let mut tails: Vec<usize> = Vec::new(); // index into candidates of the smallest tail per length
    let mut prev: Vec<Option<usize>> = vec![None; candidates.len()];
    for (k, &(_, j)) in candidates.iter().enumerate() {
        let len = tails.partition_point(|&t| candidates[t].1 < j);
        prev[k] = len.checked_sub(1).map(|l| tails[l]);
        if len == tails.len() {
            tails.push(k);
        } else {
            tails[len] = k;
        }
    }
    let mut out = Vec::new();
    let mut k = tails.last().copied();
    while let Some(i) = k {
        out.push(candidates[i]);
        k = prev[i];
    }
    out.reverse();
    out
}

/// Exact longest common subsequence (dynamic programming), for small gaps.
fn lcs(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[at(i, j)] = if a[i] == b[j] {
                t[at(i + 1, j + 1)] + 1
            } else {
                t[at(i + 1, j)].max(t[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[at(i + 1, j)] >= t[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}
