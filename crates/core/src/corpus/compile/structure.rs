//! The structure pass for plain-text editions: paragraphs → divisions, numbered sections,
//! editor headings and notes → units (for coverage) and planned passages (for the store).
//!
//! Headings are recognised in three ways, most generic first:
//! 1. known heading words: `PART 4`, `BOOK II`, `CHAPTER IX`, `INTRODUCTION`, `PREFACE`, …
//! 2. entries of the book's own CONTENTS list (the strongest signal in published research);
//! 3. per-edition overrides for unusual books (`sources.toml`).
//!
//! Anything that merely *looks* like a heading (a short all-caps line) but isn't recognised is a
//! **warning** for review, never a silent division. Structural doubts are **problems**: the
//! edition goes to review instead of being stored.

use std::collections::BTreeMap;
use std::ops::Range;

use super::paragraphs::{paragraphs, trimmed};
use super::{Compiled, DivisionNode, Unit, UnitKind, check_coverage};

/// A heading to recognise in an unusual book, and what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingOverride {
    /// The heading line exactly as printed (compared case-insensitively, whitespace collapsed).
    pub text: String,
    pub kind: String,
    pub label: String,
    /// Locator segment for passages inside it, e.g. `part-1`.
    pub slug: String,
}

/// Who wrote a style of bracketed insertion in this edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribution {
    Editor,
    Author,
}

/// Bracketed text starting with `prefix` (e.g. `[Note`, `[Vide`, or `[` for all) is by `by`.
/// The longest matching prefix wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertionRule {
    pub prefix: String,
    pub by: Attribution,
}

/// Facts about the edition known in advance (e.g. from the printed book); any mismatch is a
/// problem.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expect {
    /// Division locator segments, in order.
    pub divisions: Option<Vec<String>>,
    /// Number of author-text passages per division (e.g. `part-1` → 156 sections).
    pub text_passages: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub headings: Vec<HeadingOverride>,
    /// Who wrote each bracket style. A bracket in author text matching no rule is a problem.
    pub insertions: Vec<InsertionRule>,
    pub expect: Option<Expect>,
}

/// Whose words a planned passage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedRole {
    Text,
    /// Heading added by an editor to the passage at this index in `passages`.
    EditorHeading { of: usize },
    /// Editor's note or cross-reference on the passage at this index (a paragraph, or an
    /// insertion inside it).
    EditorNote { on: usize },
    /// Editor's (or transcriber's) bare marker printed inside the passage at this index.
    EditorMarker { in_: usize },
    /// The author's own note or reference on the passage at this index.
    AuthorNote { on: usize },
}

/// A passage the store should create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPassage {
    /// Work-level address: `part-1/3`, `part-1/3/heading`, `part-1/3/note-1`, `preface/1`.
    pub locator: String,
    pub role: PlannedRole,
    pub range: Range<usize>,
    /// Index into `Compiled::divisions`.
    pub division: usize,
}

/// Everything the compiler learned about one edition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    pub compiled: Compiled,
    pub passages: Vec<PlannedPassage>,
    /// The book's own CONTENTS entries, if it has a contents list.
    pub toc: Vec<String>,
    /// Blocking: the edition goes to review, nothing is stored.
    pub problems: Vec<String>,
    /// Non-blocking: worth a human look.
    pub warnings: Vec<String>,
}

impl Structure {
    /// The author's own words in passage `index`: its bytes minus every nested editor note or
    /// marker printed inside it (author notes stay — they're the author's). Pieces are joined
    /// with a single space where an insertion was removed.
    pub fn author_words(&self, file: &[u8], index: usize) -> String {
        let host = &self.passages[index].range;
        let mut cut: Vec<Range<usize>> = self
            .passages
            .iter()
            .filter(|p| match p.role {
                PlannedRole::EditorNote { on } | PlannedRole::EditorMarker { in_: on } => on == index,
                _ => false,
            })
            .map(|p| p.range.clone())
            .filter(|r| host.start <= r.start && r.end <= host.end)
            .collect();
        cut.sort_by_key(|r| r.start);
        let mut out = String::new();
        let mut cursor = host.start;
        for r in cut {
            out.push_str(&String::from_utf8_lossy(&file[cursor..r.start]));
            out.push(' ');
            cursor = r.end;
        }
        out.push_str(&String::from_utf8_lossy(&file[cursor..host.end]));
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// Front matter longer than this before the first heading means a heading was probably missed.
const MAX_TITLE_PAGE_BYTES: usize = 1500;
/// Passages longer than this many words get a warning (a missed boundary is likely).
const LONG_PASSAGE_WORDS: usize = 1500;

pub fn compile_plain_text(file: &[u8], body: Range<usize>, options: &Options) -> Structure {
    let paras = paragraphs(file, body.clone());
    let text = |r: &Range<usize>| String::from_utf8_lossy(&file[r.clone()]).into_owned();
    let mut problems = Vec::new();
    let mut warnings = Vec::new();

    // ---- 1. Contents list: the paragraphs after a CONTENTS heading, up to the next separator
    // or heading, become TOC entries (one per line).
    let mut toc: Vec<String> = Vec::new();
    let contents_heading = paras.iter().position(|r| normalise(&text(r)) == "CONTENTS");
    let mut toc_paragraphs = Vec::new();
    if let Some(c) = contents_heading {
        for (i, r) in paras.iter().enumerate().skip(c + 1) {
            if is_separator(&text(r)) || known_heading(&text(r)).is_some() {
                break;
            }
            toc_paragraphs.push(i);
            toc.extend(text(r).lines().map(normalise).filter(|l| !l.is_empty()));
        }
    }

    // ---- 2. Classify every paragraph as a heading, separator, contents, or content.
    #[derive(Clone)]
    enum Para {
        Heading(DivisionNode),
        Separator,
        Contents,
        Content,
    }
    let mut kinds: Vec<Para> = Vec::with_capacity(paras.len());
    for (i, r) in paras.iter().enumerate() {
        let t = text(r);
        let n = normalise(&t);
        let kind = if Some(i) == contents_heading {
            Para::Heading(division("contents", "Contents", "contents", r.clone()))
        } else if toc_paragraphs.contains(&i) {
            Para::Contents
        } else if is_separator(&t) {
            Para::Separator
        } else if let Some(o) = options.headings.iter().find(|o| normalise(&o.text) == n) {
            Para::Heading(division(&o.kind, &o.label, &o.slug, r.clone()))
        } else if let Some((kind, label, slug)) = known_heading(&t) {
            Para::Heading(division(&kind, &label, &slug, r.clone()))
        } else if contents_heading.is_some_and(|c| i > c) && toc.contains(&n) {
            // Listed in the book's own contents: a heading, even without a known heading word.
            let slug = slugify(&n);
            Para::Heading(division("division", &title_case(&n), &slug, r.clone()))
        } else {
            if looks_like_heading(&t) {
                warnings.push(format!("all-caps line not recognised as a heading (kept as text): {:?}", t.trim()));
            }
            Para::Content
        };
        kinds.push(kind);
    }

    // Every override must have matched something, or it's a typo in sources.toml.
    for o in &options.headings {
        if !paras.iter().any(|r| normalise(&text(r)) == normalise(&o.text)) {
            problems.push(format!("heading override {:?} matches no line in the text", o.text));
        }
    }

    // ---- 3. Title page: everything before the first heading.
    let first_heading = kinds.iter().position(|k| matches!(k, Para::Heading(_)));
    let mut units = Vec::new();
    let title_end = first_heading.unwrap_or(paras.len());
    let title_bytes: usize = paras[..title_end].iter().map(|r| r.len()).sum();
    if first_heading.is_none() {
        problems.push("no heading recognised anywhere: structure unknown".into());
    } else if title_bytes > MAX_TITLE_PAGE_BYTES {
        problems.push(format!(
            "{title_bytes} bytes before the first heading: a heading was probably missed (add an override)"
        ));
    }
    for r in &paras[..title_end] {
        units.push(Unit { kind: UnitKind::Skipped("title page"), range: r.clone() });
    }

    // ---- 4. Divisions: from each heading to the next. Contents is skipped, not a division.
    let mut divisions: Vec<DivisionNode> = Vec::new();
    let mut passages: Vec<PlannedPassage> = Vec::new();
    let heading_indices: Vec<usize> = (0..paras.len()).filter(|&i| matches!(kinds[i], Para::Heading(_))).collect();
    for (h, &start) in heading_indices.iter().enumerate() {
        let end = heading_indices.get(h + 1).copied().unwrap_or(paras.len());
        let Para::Heading(mut node) = kinds[start].clone() else { unreachable!() };
        units.push(Unit { kind: UnitKind::DivisionHeading, range: paras[start].clone() });
        let content: Vec<usize> = (start + 1..end)
            .filter(|&i| {
                let unit_kind = match kinds[i] {
                    Para::Separator => Some(UnitKind::Skipped("separator")),
                    Para::Contents => Some(UnitKind::Skipped("table of contents")),
                    _ => None,
                };
                if let Some(kind) = unit_kind {
                    units.push(Unit { kind, range: paras[i].clone() });
                    false
                } else {
                    true
                }
            })
            .collect();
        if node.kind == "contents" {
            continue;
        }
        node.range = paras[start].start..content.last().map_or(paras[start].end, |&i| paras[i].end);
        let d = divisions.len();
        if divisions.iter().any(|existing| existing.slug == node.slug) {
            problems.push(format!("two divisions with locator {:?}", node.slug));
        }
        divisions.push(node.clone());
        let content_ranges: Vec<Range<usize>> = content.iter().map(|&i| paras[i].clone()).collect();
        if content_ranges.is_empty() {
            problems.push(format!("division {:?} has no text", node.slug));
        } else {
            let mut sink = Sink {
                file,
                options,
                division: d,
                div_slug: &node.slug,
                units: &mut units,
                passages: &mut passages,
                problems: &mut problems,
                warnings: &mut warnings,
            };
            if is_numbered(file, &content_ranges) {
                numbered_sections(&mut sink, &content_ranges);
            } else {
                plain_paragraphs(&mut sink, &content_ranges);
            }
        }
    }

    // ---- 5. Contents cross-check: every entry must be a division heading after the list,
    // in the same order.
    if !toc.is_empty() {
        let after_contents: Vec<String> = heading_indices
            .iter()
            .filter(|&&i| contents_heading.is_some_and(|c| i > c))
            .map(|&i| normalise(&text(&paras[i])))
            .collect();
        if after_contents != toc {
            problems.push(format!("contents list {toc:?} doesn't match the headings found {after_contents:?}"));
        }
    }

    // ---- 6. Long passages: a missed boundary is likely.
    for p in &passages {
        let words = text(&p.range).split_whitespace().count();
        if words > LONG_PASSAGE_WORDS {
            warnings.push(format!("passage {} has {words} words (missed boundary?)", p.locator));
        }
    }

    // ---- 7. Declared expectations.
    if let Some(expect) = &options.expect {
        let found: Vec<String> = divisions.iter().map(|d| d.slug.clone()).collect();
        if let Some(want) = &expect.divisions
            && *want != found
        {
            problems.push(format!("expected divisions {want:?}, found {found:?}"));
        }
        for (slug, &want) in &expect.text_passages {
            let Some(d) = divisions.iter().position(|d| &d.slug == slug) else {
                problems.push(format!("expected division {slug:?} not found"));
                continue;
            };
            let have = passages.iter().filter(|p| p.division == d && p.role == PlannedRole::Text).count();
            if have != want {
                problems.push(format!("division {slug}: expected {want} text passages, found {have}"));
            }
        }
    }

    units.sort_by_key(|u| u.range.start);
    let compiled = Compiled { body, divisions, units };
    problems.extend(check_coverage(file, &compiled).iter().map(|p| p.to_string()));
    Structure { compiled, passages, toc, problems, warnings }
}

/// Where the content of one division goes: units for coverage, passages for the store.
struct Sink<'a> {
    file: &'a [u8],
    options: &'a Options,
    division: usize,
    div_slug: &'a str,
    units: &'a mut Vec<Unit>,
    passages: &'a mut Vec<PlannedPassage>,
    problems: &'a mut Vec<String>,
    warnings: &'a mut Vec<String>,
}

/// Per-host counters, so locators are unique: `…/note-1`, `…/marker-1`, `…/author-note-1`.
#[derive(Default)]
struct HostCounters {
    notes: usize,
    markers: usize,
    author_notes: usize,
    /// Bare editor markers seen in the host (heading + text) and editor note paragraphs after it.
    bare_markers: usize,
    note_paragraphs: usize,
}

impl Sink<'_> {
    fn push_passage(&mut self, locator: String, role: PlannedRole, range: Range<usize>) -> usize {
        self.passages.push(PlannedPassage { locator, role, range, division: self.division });
        self.passages.len() - 1
    }

    /// Author text `range` belonging to host passage `host`: split coverage units around
    /// bracketed insertions, and plan one nested passage per insertion.
    fn author_text(&mut self, range: Range<usize>, host: usize, counters: &mut HostCounters) {
        let host_locator = self.passages[host].locator.clone();
        let mut cursor = range.start;
        for bracket in brackets(self.file, range.clone()) {
            if let Some(piece) = trimmed(self.file, cursor..bracket.start) {
                self.units.push(Unit { kind: UnitKind::Text, range: piece });
            }
            cursor = bracket.end;
            self.units.push(Unit { kind: UnitKind::Insertion, range: bracket.clone() });
            let bytes = &self.file[bracket.clone()];
            let shown = String::from_utf8_lossy(bytes).replace("\r\n", " ");
            if bytes == b"[]" {
                // No words: a transcription artefact (e.g. a lost footnote marker), not the author's.
                self.warnings.push(format!("{host_locator}: empty brackets [] (transcription artefact?)"));
                counters.markers += 1;
                self.push_passage(format!("{host_locator}/marker-{}", counters.markers), PlannedRole::EditorMarker { in_: host }, bracket);
                continue;
            }
            match attribution(self.options, bytes) {
                None => self.problems.push(format!("{host_locator}: bracket style not declared for this edition: {shown:?}")),
                Some(Attribution::Editor) if is_bare_marker(bytes) => {
                    counters.markers += 1;
                    counters.bare_markers += 1;
                    self.push_passage(format!("{host_locator}/marker-{}", counters.markers), PlannedRole::EditorMarker { in_: host }, bracket);
                }
                Some(Attribution::Editor) => {
                    counters.notes += 1;
                    self.push_passage(format!("{host_locator}/note-{}", counters.notes), PlannedRole::EditorNote { on: host }, bracket);
                }
                Some(Attribution::Author) => {
                    counters.author_notes += 1;
                    self.push_passage(
                        format!("{host_locator}/author-note-{}", counters.author_notes),
                        PlannedRole::AuthorNote { on: host },
                        bracket,
                    );
                }
            }
        }
        if let Some(piece) = trimmed(self.file, cursor..range.end) {
            self.units.push(Unit { kind: UnitKind::Text, range: piece });
        }
    }

    /// A paragraph that is entirely one bracket group: a note paragraph on `host`.
    fn note_paragraph(&mut self, range: Range<usize>, host: usize, counters: &mut HostCounters) {
        let host_locator = self.passages[host].locator.clone();
        let bytes = &self.file[range.clone()];
        match attribution(self.options, bytes) {
            None => {
                let shown: String = String::from_utf8_lossy(bytes).chars().take(60).collect();
                self.problems.push(format!("{host_locator}: note style not declared for this edition: {shown:?}"));
                self.units.push(Unit { kind: UnitKind::EditorNote, range });
            }
            Some(Attribution::Editor) => {
                counters.notes += 1;
                counters.note_paragraphs += 1;
                self.units.push(Unit { kind: UnitKind::EditorNote, range: range.clone() });
                self.push_passage(format!("{host_locator}/note-{}", counters.notes), PlannedRole::EditorNote { on: host }, range);
            }
            Some(Attribution::Author) => {
                counters.author_notes += 1;
                self.units.push(Unit { kind: UnitKind::Insertion, range: range.clone() });
                self.push_passage(
                    format!("{host_locator}/author-note-{}", counters.author_notes),
                    PlannedRole::AuthorNote { on: host },
                    range,
                );
            }
        }
    }

    /// Bare editor markers must match editor note paragraphs one-to-one within a host.
    fn close_host(&mut self, host: Option<usize>, counters: &HostCounters) {
        if let Some(h) = host
            && counters.bare_markers != counters.note_paragraphs
        {
            let locator = self.passages[h].locator.clone();
            self.problems.push(format!(
                "{locator}: {} note marker(s) but {} note paragraph(s)",
                counters.bare_markers, counters.note_paragraphs
            ));
        }
    }
}

/// Numbered sections ("1. …", "2. …"): a section is its numbered paragraph plus any following
/// unnumbered paragraphs; whole-bracket paragraphs are notes on the current section.
fn numbered_sections(sink: &mut Sink, paras: &[Range<usize>]) {
    let file = sink.file;
    let div_slug = sink.div_slug.to_owned();
    let mut expected = 1u32;
    let mut current: Option<usize> = None;
    let mut counters = HostCounters::default();
    let mut number = 0u32;

    for r in paras {
        let bytes = &file[r.clone()];
        if is_whole_bracket(bytes) {
            let Some(host) = current else {
                sink.problems.push(format!("{div_slug}: note before the first section"));
                sink.units.push(Unit { kind: UnitKind::EditorNote, range: r.clone() });
                continue;
            };
            sink.note_paragraph(r.clone(), host, &mut counters);
        } else if let Some((n, number_end)) = section_number(bytes) {
            sink.close_host(current, &counters);
            counters = HostCounters::default();
            if n != expected {
                sink.problems.push(format!("{div_slug}: section {n} where {expected} was expected"));
            }
            expected = n + 1;
            number = n;

            let number_range = r.start..r.start + number_end;
            sink.units.push(Unit { kind: UnitKind::SectionNumber, range: number_range.clone() });
            let Some(rest) = trimmed(file, number_range.end..r.end) else {
                sink.problems.push(format!("{div_slug}/{n}: section has a number but no text"));
                current = None;
                continue;
            };
            let heading = editor_heading_len(&file[rest.clone()])
                .and_then(|len| trimmed(file, rest.start..rest.start + len).zip(trimmed(file, rest.start + len..rest.end)));
            let (heading, text) = match heading {
                Some((h, t)) => (Some(h), t),
                None => (None, rest),
            };
            // The text passage first, so the heading can point at it.
            let host = sink.push_passage(format!("{div_slug}/{n}"), PlannedRole::Text, text.clone());
            if let Some(h) = heading {
                counters.bare_markers += count_bare_markers(&file[h.clone()]);
                sink.units.push(Unit { kind: UnitKind::EditorHeading, range: h.clone() });
                sink.push_passage(format!("{div_slug}/{n}/heading"), PlannedRole::EditorHeading { of: host }, h);
            }
            sink.author_text(text, host, &mut counters);
            current = Some(host);
        } else {
            // An unnumbered paragraph continues the current section.
            let Some(host) = current else {
                let shown: String = String::from_utf8_lossy(bytes).chars().take(60).collect();
                sink.problems.push(format!("{div_slug}: text before the first numbered section: {shown:?}"));
                sink.units.push(Unit { kind: UnitKind::Text, range: r.clone() });
                continue;
            };
            if counters.note_paragraphs > 0 {
                sink.problems.push(format!("{div_slug}/{number}: text continues after a note"));
            }
            sink.passages[host].range.end = r.end;
            sink.author_text(r.clone(), host, &mut counters);
        }
    }
    sink.close_host(current, &counters);
}

/// Unnumbered prose: each paragraph is a passage, `<division>/1`, `<division>/2`, …
fn plain_paragraphs(sink: &mut Sink, paras: &[Range<usize>]) {
    let div_slug = sink.div_slug.to_owned();
    let mut k = 0;
    let mut current: Option<usize> = None;
    let mut counters = HostCounters::default();
    for r in paras {
        if is_whole_bracket(&sink.file[r.clone()]) {
            let Some(host) = current else {
                sink.problems.push(format!("{div_slug}: note before any text"));
                sink.units.push(Unit { kind: UnitKind::EditorNote, range: r.clone() });
                continue;
            };
            sink.note_paragraph(r.clone(), host, &mut counters);
            continue;
        }
        sink.close_host(current, &counters);
        counters = HostCounters::default();
        k += 1;
        let host = sink.push_passage(format!("{div_slug}/{k}"), PlannedRole::Text, r.clone());
        sink.author_text(r.clone(), host, &mut counters);
        current = Some(host);
    }
    sink.close_host(current, &counters);
}

/// Who wrote this bracketed text, by the longest matching declared prefix.
fn attribution(options: &Options, bytes: &[u8]) -> Option<Attribution> {
    options
        .insertions
        .iter()
        .filter(|rule| bytes.starts_with(rule.prefix.as_bytes()))
        .max_by_key(|rule| rule.prefix.len())
        .map(|rule| rule.by)
}

/// Every `[…]` group in `range` (balanced, not nested; unbalanced brackets are left as text and
/// caught by review).
fn brackets(file: &[u8], range: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut open = None;
    for (i, &b) in file[range.clone()].iter().enumerate() {
        let at = range.start + i;
        match (b, open) {
            (b'[', None) => open = Some(at),
            (b']', Some(start)) => {
                out.push(start..at + 1);
                open = None;
            }
            _ => {}
        }
    }
    out
}

/// A paragraph that is entirely one bracket group: "[Note: …]".
fn is_whole_bracket(bytes: &[u8]) -> bool {
    bytes.first() == Some(&b'[') && bytes.last() == Some(&b']') && bytes[1..bytes.len() - 1].iter().all(|&b| b != b'[' && b != b']')
}

/// "[Note]", "[Note.]", "[Note 1.]", "[Note. 2]": a marker with no content.
fn is_bare_marker(bytes: &[u8]) -> bool {
    bytes.starts_with(b"[Note") && bytes.ends_with(b"]") && bytes[5..bytes.len() - 1].iter().all(|b| b.is_ascii_digit() || *b == b'.' || *b == b' ')
}

fn count_bare_markers(bytes: &[u8]) -> usize {
    brackets(bytes, 0..bytes.len()).into_iter().filter(|r| is_bare_marker(&bytes[r.clone()])).count()
}

/// A division uses numbered sections if its first paragraph starts "1." and at least two
/// paragraphs are numbered.
fn is_numbered(file: &[u8], paras: &[Range<usize>]) -> bool {
    let numbers: Vec<u32> = paras.iter().filter_map(|r| section_number(&file[r.clone()]).map(|(n, _)| n)).collect();
    numbers.len() >= 2 && numbers[0] == 1 && section_number(&file[paras[0].clone()]).is_some()
}

/// "35. THE…" → (35, length of "35."). Up to 4 digits, then '.', then whitespace.
fn section_number(bytes: &[u8]) -> Option<(u32, usize)> {
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || digits > 4 || bytes.get(digits) != Some(&b'.') || !bytes.get(digits + 1).is_some_and(u8::is_ascii_whitespace) {
        return None;
    }
    let n = std::str::from_utf8(&bytes[..digits]).ok()?.parse().ok()?;
    Some((n, digits + 1))
}

/// Length of an editor heading at the start of `text`: one or more segments ending in "--",
/// each of which, with bracketed parts removed, has capitals and no lowercase letters.
/// `FIRST GENERAL OBJECTION.--ANSWER.--I do…` → up to and including the second "--".
fn editor_heading_len(text: &[u8]) -> Option<usize> {
    const MAX: usize = 400;
    let mut end = 0;
    loop {
        let rest = &text[end..];
        let Some(dash) = rest.windows(2).position(|w| w == b"--") else { break };
        if end + dash > MAX {
            break;
        }
        let segment = without_brackets(&rest[..dash]);
        let has_upper = segment.iter().any(u8::is_ascii_uppercase);
        let has_lower = segment.iter().any(u8::is_ascii_lowercase);
        if !has_upper || has_lower {
            break;
        }
        end += dash + 2;
    }
    (end > 0).then_some(end)
}

/// `bytes` with every `[…]` group removed.
fn without_brackets(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    for &b in bytes {
        match b {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(b),
            _ => {}
        }
    }
    out
}

/// `PART 4`, `PART IV`, `BOOK II`, `CHAPTER IX`, `SECTION 3`, or a standalone known heading word.
/// Returns (kind, label, slug).
fn known_heading(text: &str) -> Option<(String, String, String)> {
    let n = normalise(text);
    if text.trim().lines().count() != 1 {
        return None;
    }
    for kind in ["PART", "BOOK", "CHAPTER", "SECTION"] {
        if let Some(rest) = n.strip_prefix(kind).and_then(|r| r.strip_prefix(' ')) {
            let number = rest.trim_end_matches('.');
            let value = number.parse::<u32>().ok().or_else(|| roman(number))?;
            let lower = kind.to_lowercase();
            return Some((lower.clone(), number.to_owned(), format!("{lower}-{value}")));
        }
    }
    let words = ["INTRODUCTION", "PREFACE", "DEDICATION", "APPENDIX", "CONCLUSION", "ADVERTISEMENT", "PROLOGUE", "EPILOGUE"];
    words.iter().find(|w| n == **w).map(|w| {
        let lower = w.to_lowercase();
        (lower.clone(), title_case(w), lower)
    })
}

/// Roman numerals I–MMMM (validated by round-tripping through the canonical form).
fn roman(s: &str) -> Option<u32> {
    let value = |c| match c {
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
    if digits.is_empty() {
        return None;
    }
    let mut total = 0;
    for (i, &d) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|&next| next > d) { total -= d as i64 } else { total += d as i64 }
    }
    let total = u32::try_from(total).ok().filter(|&t| t > 0)?;
    (to_roman(total) == s).then_some(total)
}

fn to_roman(mut n: u32) -> String {
    let table = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut out = String::new();
    for (v, s) in table {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

/// A short single-line paragraph with capitals and no lowercase letters.
fn looks_like_heading(text: &str) -> bool {
    let t = text.trim();
    t.lines().count() == 1 && t.len() <= 80 && t.chars().any(|c| c.is_ascii_uppercase()) && !t.chars().any(|c| c.is_ascii_lowercase())
}

/// "* * * * *"
fn is_separator(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && t.chars().all(|c| c == '*' || c == ' ') && t.contains('*')
}

/// Uppercase, whitespace collapsed: for comparing heading lines.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase()
}

/// "OF THE PRINCIPLES OF HUMAN KNOWLEDGE" → "of-the-principles-of-human-knowledge" (ASCII only).
fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()) {
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word.to_ascii_lowercase());
    }
    slug
}

fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| f.to_string() + &c.as_str().to_lowercase())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn division(kind: &str, label: &str, slug: &str, range: Range<usize>) -> DivisionNode {
    DivisionNode { kind: kind.into(), label: label.into(), slug: slug.into(), range, children: vec![] }
}

