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
    /// Whose words the division is, if not the author's: `Editor` (a modern introduction) is
    /// covered but never passages.
    pub by: Option<Attribution>,
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
    /// Author-text passages in the whole edition, including any with no division (Gaunilo's
    /// eight sections).
    pub text_passages_total: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub headings: Vec<HeadingOverride>,
    /// What one passage is (declared per edition when the text can't tell).
    pub passages: PassageUnit,
    /// Who wrote each bracket style. A bracket in author text matching no rule is a problem.
    pub insertions: Vec<InsertionRule>,
    /// How this edition prints the author's marginal notes, if it does.
    pub margin_notes: MarginNotes,
    pub expect: Option<Expect>,
}

/// How an edition prints the author's marginal notes (side-notes). Declared, never guessed: a
/// short paragraph in Title Case could be anything in an edition that doesn't print notes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarginNotes {
    /// No marginal notes in this edition.
    #[default]
    None,
    /// As separate paragraphs of up to three lines, every word capitalised, each before the
    /// paragraph it glosses (Gutenberg's Leviathan: "Apparitions Or Visions").
    TitleCase,
}

/// What one citable passage is. Books differ in ways the page doesn't show: Berkeley's numbered
/// sections are the citation unit (PHK 1.3, continuation paragraphs included), while Hume's
/// Enquiry is cited by paragraph (EHU 10.1) and its printed numbers are Selby-Bigge's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PassageUnit {
    /// Numbered sections if a division's content starts numbered, otherwise paragraphs.
    #[default]
    Auto,
    Paragraphs,
    NumberedSections,
}

/// Whose words a planned passage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedRole {
    Text,
    /// Heading added by an editor to the passage at this index in `passages`.
    EditorHeading {
        of: usize,
    },
    /// Editor's note or cross-reference on the passage at this index (a paragraph, or an
    /// insertion inside it).
    EditorNote {
        on: usize,
    },
    /// Editor's (or transcriber's) bare marker printed inside the passage at this index.
    EditorMarker {
        in_: usize,
    },
    /// The author's own note or reference on the passage at this index.
    AuthorNote {
        on: usize,
    },
    /// The author's marginal note glossing the passage at this index.
    MarginNote {
        on: usize,
    },
}

/// A passage the store should create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPassage {
    /// Work-level address: `part-1/3`, `part-1/3/heading`, `part-1/3/note-1`, `preface/1`.
    pub locator: String,
    pub role: PlannedRole,
    pub range: Range<usize>,
    /// Index into `Compiled::divisions`, or `None` for a work with no divisions (Gaunilo's
    /// reply, cited by its numbered sections alone).
    pub division: Option<usize>,
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
    /// The file is markup (XML): passage bytes include tags, and author words leave them out.
    pub markup: bool,
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
                PlannedRole::EditorNote { on } | PlannedRole::EditorMarker { in_: on } => {
                    on == index
                }
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
        if self.markup {
            out = super::thml::text_of(&out);
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// Declared expectations (`[document.edition.expect]`): the division paths, and the text passages
/// in each named division.
pub(super) fn check_expectations(
    options: &Options,
    paths: &[String],
    passages: &[PlannedPassage],
    problems: &mut Vec<String>,
) {
    if let Some(want) = options.expect.as_ref().and_then(|e| e.text_passages_total) {
        let have = passages
            .iter()
            .filter(|p| p.role == PlannedRole::Text)
            .count();
        if have != want {
            problems.push(format!(
                "expected {want} text passages in all, found {have}"
            ));
        }
    }
    if let Some(expect) = &options.expect {
        let found: Vec<String> = paths.to_vec();
        if let Some(want) = &expect.divisions
            && *want != found
        {
            problems.push(format!("expected divisions {want:?}, found {found:?}"));
        }
        for (slug, &want) in &expect.text_passages {
            let Some(d) = paths.iter().position(|p| p == slug) else {
                problems.push(format!("expected division {slug:?} not found"));
                continue;
            };
            let have = passages
                .iter()
                .filter(|p| p.division == Some(d) && p.role == PlannedRole::Text)
                .count();
            if have != want {
                problems.push(format!(
                    "division {slug}: expected {want} text passages, found {have}"
                ));
            }
        }
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
    let contents_heading = paras.iter().position(|r| is_contents_heading(&text(r)));
    let mut toc_paragraphs = Vec::new();
    if let Some(c) = contents_heading {
        // The list runs to the first separator or heading-like line. If that line is an entry
        // of the list ("PART I.", "INTRODUCTION"), the same heading appears again where the body
        // starts. With no prose in between (a contents list has none), the list runs to there.
        // With front matter in between (a preface, a dedication), the list is its entries: it
        // runs while paragraphs are headings that appear again later, or other lines without
        // lowercase ("PAGE" / "INTRODUCTORY 1"), and ends at the first that isn't.
        let id = |i: usize| known_heading(&text(&paras[i])).map(|k| (k.kind, k.slug));
        let repeats = |i: usize| (i + 1..paras.len()).find(|&j| id(j) == id(i));
        let stop = (c + 1..paras.len())
            .find(|&i| is_separator(&text(&paras[i])) || id(i).is_some())
            .unwrap_or(paras.len());
        let end = match (stop < paras.len() && id(stop).is_some())
            .then(|| repeats(stop))
            .flatten()
        {
            Some(body) if !(stop + 1..body).any(|i| is_prose(&text(&paras[i]))) => body,
            Some(_) => {
                let mut end = (stop..paras.len())
                    .find(|&i| {
                        let t = text(&paras[i]);
                        let entry = match id(i) {
                            Some(_) => repeats(i).is_some(),
                            None => {
                                !t.chars().any(char::is_lowercase)
                                    || (t.trim().lines().count() <= 3
                                        && t.lines().next().and_then(numbered_entry).is_some())
                            }
                        };
                        !entry || is_separator(&text(&paras[i]))
                    })
                    .unwrap_or(paras.len());
                // A title in capitals opening the front matter ("A BRIEF SKETCH OF THE LIFE…")
                // looks like an entry; an entry that isn't a heading is numbered ("CHAP. I. …")
                // or ends in a page number.
                while end > stop
                    && id(end - 1).is_none()
                    && !ends_in_page_number(&text(&paras[end - 1]))
                    && text(&paras[end - 1])
                        .lines()
                        .next()
                        .and_then(numbered_entry)
                        .is_none()
                {
                    end -= 1;
                }
                end
            }
            None => stop,
        };
        for (i, r) in paras.iter().enumerate().take(end).skip(c + 1) {
            toc_paragraphs.push(i);
            toc.extend(text(r).lines().map(normalise).filter(|l| !l.is_empty()));
        }
    }

    // ---- 2. Classify every paragraph as a heading, separator, contents, or content.
    #[derive(Clone)]
    enum Para {
        Heading(DivisionNode),
        Separator,
        EndMark,
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
        } else if is_end_mark(&t) {
            Para::EndMark
        } else if let Some(o) = options.headings.iter().find(|o| normalise(&o.text) == n) {
            Para::Heading(division(&o.kind, &o.label, &o.slug, r.clone()))
        } else if let Some(k) = known_heading(&t) {
            let mut node = division(&k.kind, &k.label, &k.slug, r.clone());
            node.title = k.title;
            Para::Heading(node)
        } else if contents_heading.is_some_and(|c| i > c) && toc.contains(&n) {
            // Listed in the book's own contents: a heading, even without a known heading word.
            let slug = slugify(&n);
            Para::Heading(division("division", &title_case(&n), &slug, r.clone()))
        } else {
            if looks_like_heading(&t) {
                warnings.push(format!(
                    "all-caps line not recognised as a heading (kept as text): {:?}",
                    t.trim()
                ));
            }
            Para::Content
        };
        kinds.push(kind);
    }

    // Every override must have matched something, or it's a typo in sources.toml.
    for o in &options.headings {
        if !paras
            .iter()
            .any(|r| normalise(&text(r)) == normalise(&o.text))
        {
            problems.push(format!(
                "heading override {:?} matches no line in the text",
                o.text
            ));
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
        units.push(Unit {
            kind: UnitKind::Skipped("title page"),
            range: r.clone(),
        });
    }

    // ---- 4. Divisions: from each heading to the next. Contents is skipped, not a division.
    let mut divisions: Vec<DivisionNode> = Vec::new();
    // Each division's full locator path ("section-4/part-1"), parallel to `divisions`.
    let mut paths: Vec<String> = Vec::new();
    // Open numbered divisions, outermost first, for nesting (see `nest`).
    let mut open: Vec<usize> = Vec::new();
    // Divisions with no text of their own: fine if they hold sub-divisions (checked at the end).
    let mut without_text: Vec<usize> = Vec::new();
    let mut passages: Vec<PlannedPassage> = Vec::new();
    let heading_indices: Vec<usize> = (0..paras.len())
        .filter(|&i| matches!(kinds[i], Para::Heading(_)))
        .collect();
    for (h, &start) in heading_indices.iter().enumerate() {
        let end = heading_indices.get(h + 1).copied().unwrap_or(paras.len());
        let Para::Heading(mut node) = kinds[start].clone() else {
            unreachable!()
        };
        units.push(Unit {
            kind: UnitKind::DivisionHeading,
            range: paras[start].clone(),
        });
        let content: Vec<usize> = (start + 1..end)
            .filter(|&i| {
                let unit_kind = match kinds[i] {
                    Para::Separator => Some(UnitKind::Skipped("separator")),
                    Para::EndMark => Some(UnitKind::Skipped("end mark")),
                    Para::Contents => Some(UnitKind::Skipped("table of contents")),
                    _ => None,
                };
                if let Some(kind) = unit_kind {
                    units.push(Unit {
                        kind,
                        range: paras[i].clone(),
                    });
                    false
                } else {
                    true
                }
            })
            .collect();
        if node.kind == "contents" {
            // Front matter between the list and the next heading (a life of the author, a
            // dedication, an epigraph): whose words it is can't be told, so it's skipped like a
            // title page — and if it's long, a heading was probably missed (add an override).
            let bytes: usize = content.iter().map(|&i| paras[i].len()).sum();
            if bytes > MAX_TITLE_PAGE_BYTES {
                problems.push(format!(
                    "{bytes} bytes of front matter after the contents list: a heading was probably missed (add an override)"
                ));
            }
            for &i in &content {
                units.push(Unit {
                    kind: UnitKind::Skipped("front matter"),
                    range: paras[i].clone(),
                });
            }
            continue;
        }
        // A division declared as the editor's (a modern introduction): covered, never passages.
        let editorial = options.headings.iter().any(|o| {
            o.by == Some(Attribution::Editor)
                && normalise(&o.text) == normalise(&text(&paras[start]))
        });
        if editorial {
            for &i in &content {
                units.push(Unit {
                    kind: UnitKind::Skipped("editor's division"),
                    range: paras[i].clone(),
                });
            }
            continue;
        }
        // A back-of-book index is the publisher's apparatus: covered, but never passages.
        if node.kind == "index" {
            for &i in &content {
                units.push(Unit {
                    kind: UnitKind::Skipped("index"),
                    range: paras[i].clone(),
                });
            }
            continue;
        }
        node.range = paras[start].start..content.last().map_or(paras[start].end, |&i| paras[i].end);
        let d = divisions.len();
        // Directly under a heading with no text of its own (Smith's "SECTION II." then
        // "INTRODUCTION.")?
        let under_bare_heading = d > 0 && without_text.last() == Some(&(d - 1));
        node.parent = nest(&mut open, &divisions, &node, d, under_bare_heading);
        let path = match node.parent {
            Some(p) => format!("{}/{}", paths[p], node.slug),
            None => node.slug.clone(),
        };
        if paths.contains(&path) {
            problems.push(format!("two divisions with locator {path:?}"));
        }
        // A nested division ends where its parent does, at the latest: extend the parents.
        let mut up = node.parent;
        while let Some(p) = up {
            divisions[p].range.end = divisions[p].range.end.max(node.range.end);
            up = divisions[p].parent;
        }
        divisions.push(node.clone());
        paths.push(path.clone());
        let mut content_ranges: Vec<Range<usize>> =
            content.iter().map(|&i| paras[i].clone()).collect();
        // A title printed on the heading line may wrap over a blank line into a paragraph in
        // capitals ("CHAPTER VI. OF THE INTERIOUR BEGINNINGS OF VOLUNTARY MOTIONS" /
        // "COMMONLY CALLED THE PASSIONS. …").
        if known_heading(&text(&paras[start])).is_some_and(|k| k.title_on_heading_line)
            && let Some(first) = content_ranges.first().cloned()
            && is_title_continuation(&text(&first))
        {
            let title = format!("{} {}", node.title.as_deref().unwrap_or(""), text(&first));
            node.title = Some(title_text(&title));
            divisions[d].title = node.title.clone();
            units.push(Unit {
                kind: UnitKind::DivisionHeading,
                range: first,
            });
            content_ranges.remove(0);
        }
        // Heading material after a heading that already carries its title: a lone short line
        // before the sub-divisions ("Consisting of three Sections."), so the next heading is of
        // another kind (a chapter whose only text is one short line keeps it).
        let next_kind = heading_indices.get(h + 1).and_then(|&i| match &kinds[i] {
            Para::Heading(next) => Some(next.kind.clone()),
            _ => None,
        });
        if node.title.is_some()
            && next_kind.is_some_and(|k| k != node.kind)
            && let [only] = &content_ranges[..]
            && text(only).trim().lines().count() == 1
            && text(only).trim().len() <= 80
        {
            units.push(Unit {
                kind: UnitKind::DivisionHeading,
                range: only.clone(),
            });
            content_ranges.clear();
        }
        // A numbered division's title: its first paragraph, up to three short lines in capitals.
        let mut title_markers = Vec::new();
        if node.title.is_none()
            && heading_number(&node.label).is_some()
            && let Some(first) = content_ranges.first().cloned()
            && (is_title_continuation(&text(&first)) || is_italic_title(&text(&first)))
            && footnote_number(&file[first.clone()]).is_none()
            && section_number(&file[first.clone()]).is_none()
        {
            let printed = text(&first);
            title_markers = bracket_numbers(printed.as_bytes());
            let title = String::from_utf8_lossy(&without_brackets(printed.as_bytes())).into_owned();
            let title = title_text(&title);
            divisions[d].title = Some(title.clone());
            node.title = Some(title);
            units.push(Unit {
                kind: UnitKind::DivisionHeading,
                range: first,
            });
            content_ranges.remove(0);
        }
        if content_ranges.is_empty() {
            without_text.push(d);
        } else {
            let mut sink = Sink {
                file,
                options,
                division: d,
                div_slug: &path,
                title_markers: &title_markers,
                units: &mut units,
                passages: &mut passages,
                problems: &mut problems,
                warnings: &mut warnings,
            };
            let numbered = match options.passages {
                PassageUnit::Auto => is_numbered(file, &content_ranges),
                PassageUnit::Paragraphs => false,
                PassageUnit::NumberedSections => true,
            };
            if numbered {
                numbered_sections(&mut sink, &content_ranges);
            } else {
                plain_paragraphs(&mut sink, &content_ranges);
            }
        }
    }

    for d in without_text {
        if !divisions.iter().any(|x| x.parent == Some(d)) {
            problems.push(format!("division {:?} has no text", paths[d]));
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
        // Lists print page numbers and wrap entries over lines, so failing the exact match,
        // the list's numbered entries (Part I, Chap. I, Chap. II, …) must match the numbered
        // headings found, in order.
        let mut listed: Vec<(&str, u32)> = toc.iter().filter_map(|l| numbered_entry(l)).collect();
        // The same entry twice in a row is a transcription slip (no book has two consecutive
        // Chapter VIIs): kept visible as a warning.
        let printed = listed.len();
        listed.dedup();
        if listed.len() < printed {
            warnings.push(format!(
                "contents list repeats {} entr(y/ies) in a row (transcription slip?)",
                printed - listed.len()
            ));
        }
        let found: Vec<(&str, u32)> = after_contents
            .iter()
            .filter_map(|h| h.lines().next().and_then(numbered_entry))
            .filter(|(kind, _)| listed.iter().any(|(k, _)| k == kind))
            .collect();
        if after_contents != toc && (listed.is_empty() || listed != found) {
            problems.push(format!(
                "contents list {toc:?} doesn't match the headings found {after_contents:?}"
            ));
        }
    }

    // ---- 6. Long passages: a missed boundary is likely.
    for p in &passages {
        let words = text(&p.range).split_whitespace().count();
        if words > LONG_PASSAGE_WORDS {
            warnings.push(format!(
                "passage {} has {words} words (missed boundary?)",
                p.locator
            ));
        }
    }

    // ---- 7. Declared expectations.
    check_expectations(options, &paths, &passages, &mut problems);

    units.sort_by_key(|u| u.range.start);
    let compiled = Compiled {
        body,
        divisions,
        units,
    };
    problems.extend(
        check_coverage(file, &compiled)
            .iter()
            .map(|p| p.to_string()),
    );
    Structure {
        compiled,
        passages,
        toc,
        problems,
        warnings,
        markup: false,
    }
}

/// Where the content of one division goes: units for coverage, passages for the store.
struct Sink<'a> {
    file: &'a [u8],
    options: &'a Options,
    division: usize,
    div_slug: &'a str,
    /// Footnote numbers whose marker is in the division's title ("OF PROBABILITY[9].").
    title_markers: &'a [u32],
    units: &'a mut Vec<Unit>,
    passages: &'a mut Vec<PlannedPassage>,
    problems: &'a mut Vec<String>,
    warnings: &'a mut Vec<String>,
}

/// Where a footnote paragraph belongs: the text passage with its marker, or the division's title.
#[derive(Debug, Clone, Copy)]
enum NoteHost {
    Passage(usize),
    Title(u32),
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
        self.passages.push(PlannedPassage {
            locator,
            role,
            range,
            division: Some(self.division),
        });
        self.passages.len() - 1
    }

    /// Author text `range` belonging to host passage `host`: split coverage units around
    /// bracketed insertions, and plan one nested passage per insertion.
    fn author_text(&mut self, range: Range<usize>, host: usize, counters: &mut HostCounters) {
        let host_locator = self.passages[host].locator.clone();
        let mut cursor = range.start;
        for bracket in brackets(self.file, range.clone()) {
            if let Some(piece) = trimmed(self.file, cursor..bracket.start) {
                self.units.push(Unit {
                    kind: UnitKind::Text,
                    range: piece,
                });
            }
            cursor = bracket.end;
            self.units.push(Unit {
                kind: UnitKind::Insertion,
                range: bracket.clone(),
            });
            let bytes = &self.file[bracket.clone()];
            let shown = String::from_utf8_lossy(bytes).replace("\r\n", " ");
            if bytes == b"[]" {
                // No words: a transcription artefact (e.g. a lost footnote marker), not the author's.
                self.warnings.push(format!(
                    "{host_locator}: empty brackets [] (transcription artefact?)"
                ));
                counters.markers += 1;
                self.push_passage(
                    format!("{host_locator}/marker-{}", counters.markers),
                    PlannedRole::EditorMarker { in_: host },
                    bracket,
                );
                continue;
            }
            match attribution(self.options, bytes) {
                None => self.problems.push(format!(
                    "{host_locator}: bracket style not declared for this edition: {shown:?}"
                )),
                Some(Attribution::Editor) if is_bare_marker(bytes) => {
                    counters.markers += 1;
                    counters.bare_markers += 1;
                    self.push_passage(
                        format!("{host_locator}/marker-{}", counters.markers),
                        PlannedRole::EditorMarker { in_: host },
                        bracket,
                    );
                }
                Some(Attribution::Editor) => {
                    counters.notes += 1;
                    self.push_passage(
                        format!("{host_locator}/note-{}", counters.notes),
                        PlannedRole::EditorNote { on: host },
                        bracket,
                    );
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
            self.units.push(Unit {
                kind: UnitKind::Text,
                range: piece,
            });
        }
    }

    /// A paragraph that is entirely one bracket group: a note paragraph on `host`.
    fn note_paragraph(&mut self, range: Range<usize>, host: usize, counters: &mut HostCounters) {
        let host_locator = self.passages[host].locator.clone();
        let bytes = &self.file[range.clone()];
        match attribution(self.options, bytes) {
            None => {
                let shown: String = String::from_utf8_lossy(bytes).chars().take(60).collect();
                self.problems.push(format!(
                    "{host_locator}: note style not declared for this edition: {shown:?}"
                ));
                self.units.push(Unit {
                    kind: UnitKind::EditorNote,
                    range,
                });
            }
            Some(Attribution::Editor) => {
                counters.notes += 1;
                counters.note_paragraphs += 1;
                self.units.push(Unit {
                    kind: UnitKind::EditorNote,
                    range: range.clone(),
                });
                self.push_passage(
                    format!("{host_locator}/note-{}", counters.notes),
                    PlannedRole::EditorNote { on: host },
                    range,
                );
            }
            Some(Attribution::Author) => {
                counters.author_notes += 1;
                self.units.push(Unit {
                    kind: UnitKind::Insertion,
                    range: range.clone(),
                });
                self.push_passage(
                    format!("{host_locator}/author-note-{}", counters.author_notes),
                    PlannedRole::AuthorNote { on: host },
                    range,
                );
            }
        }
    }

    /// A footnote printed as a paragraph: "[n] …" whose marker "[n]" appears in a text passage of
    /// this division, plus the indented paragraphs that continue it. Planned as note paragraphs
    /// on the passage with the marker (attribution as declared). `open` is the host of the
    /// footnote being continued. Returns whether `r` was a footnote paragraph.
    fn footnote(
        &mut self,
        r: &Range<usize>,
        open: &mut Option<NoteHost>,
        counters: &mut HostCounters,
    ) -> bool {
        let file = self.file;
        let host = match footnote_number(&file[r.clone()]) {
            Some(n) => {
                let marker = format!("[{n}]");
                let passage = self
                    .passages
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, p)| {
                        p.division == Some(self.division)
                            && p.role == PlannedRole::Text
                            && file[p.range.clone()]
                                .windows(marker.len())
                                .any(|w| w == marker.as_bytes())
                    })
                    .map(|(i, _)| NoteHost::Passage(i));
                passage.or_else(|| {
                    self.title_markers
                        .contains(&n)
                        .then_some(NoteHost::Title(n))
                })
            }
            None => open.filter(|_| indented(file, r)),
        };
        *open = host;
        match host {
            None => false,
            Some(NoteHost::Passage(host)) => {
                self.note_paragraph(r.clone(), host, counters);
                true
            }
            Some(NoteHost::Title(n)) => {
                // Notes attach to passages; a note on a heading has nowhere to go yet.
                if footnote_number(&file[r.clone()]).is_some() {
                    self.problems.push(format!(
                        "{}: footnote on a heading ([{n}]) can't be stored yet",
                        self.div_slug
                    ));
                }
                self.units.push(Unit {
                    kind: UnitKind::EditorNote,
                    range: r.clone(),
                });
                true
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

    let mut footnote: Option<NoteHost> = None;
    for r in paras {
        let bytes = &file[r.clone()];
        if sink.footnote(r, &mut footnote, &mut counters) {
            continue;
        }
        if is_whole_bracket(bytes) {
            let Some(host) = current else {
                sink.problems
                    .push(format!("{div_slug}: note before the first section"));
                sink.units.push(Unit {
                    kind: UnitKind::EditorNote,
                    range: r.clone(),
                });
                continue;
            };
            sink.note_paragraph(r.clone(), host, &mut counters);
        } else if let Some((n, number_end)) = section_number(bytes) {
            sink.close_host(current, &counters);
            counters = HostCounters::default();
            if n != expected {
                sink.problems.push(format!(
                    "{div_slug}: section {n} where {expected} was expected"
                ));
            }
            expected = n + 1;
            number = n;

            let number_range = r.start..r.start + number_end;
            sink.units.push(Unit {
                kind: UnitKind::SectionNumber,
                range: number_range.clone(),
            });
            let Some(rest) = trimmed(file, number_range.end..r.end) else {
                sink.problems
                    .push(format!("{div_slug}/{n}: section has a number but no text"));
                current = None;
                continue;
            };
            let heading = editor_heading_len(&file[rest.clone()]).and_then(|len| {
                trimmed(file, rest.start..rest.start + len)
                    .zip(trimmed(file, rest.start + len..rest.end))
            });
            let (heading, text) = match heading {
                Some((h, t)) => (Some(h), t),
                None => (None, rest),
            };
            // The text passage first, so the heading can point at it.
            let host =
                sink.push_passage(format!("{div_slug}/{n}"), PlannedRole::Text, text.clone());
            if let Some(h) = heading {
                counters.bare_markers += count_bare_markers(&file[h.clone()]);
                sink.units.push(Unit {
                    kind: UnitKind::EditorHeading,
                    range: h.clone(),
                });
                sink.push_passage(
                    format!("{div_slug}/{n}/heading"),
                    PlannedRole::EditorHeading { of: host },
                    h,
                );
            }
            sink.author_text(text, host, &mut counters);
            current = Some(host);
        } else {
            // An unnumbered paragraph continues the current section.
            let Some(host) = current else {
                let shown: String = String::from_utf8_lossy(bytes).chars().take(60).collect();
                sink.problems.push(format!(
                    "{div_slug}: text before the first numbered section: {shown:?}"
                ));
                sink.units.push(Unit {
                    kind: UnitKind::Text,
                    range: r.clone(),
                });
                continue;
            };
            if counters.note_paragraphs > 0 {
                sink.problems
                    .push(format!("{div_slug}/{number}: text continues after a note"));
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
    let mut footnote: Option<NoteHost> = None;
    // Margin notes printed before the paragraph they gloss, waiting for it. They're planned
    // after it: the store needs an attachment's target first.
    let mut margin_notes: Vec<Range<usize>> = Vec::new();
    for r in paras {
        if sink.footnote(r, &mut footnote, &mut counters) {
            continue;
        }
        if sink.options.margin_notes == MarginNotes::TitleCase
            && is_title_case_note(&String::from_utf8_lossy(&sink.file[r.clone()]))
        {
            sink.units.push(Unit {
                kind: UnitKind::MarginNote,
                range: r.clone(),
            });
            margin_notes.push(r.clone());
            continue;
        }
        if is_whole_bracket(&sink.file[r.clone()]) {
            let Some(host) = current else {
                sink.problems
                    .push(format!("{div_slug}: note before any text"));
                sink.units.push(Unit {
                    kind: UnitKind::EditorNote,
                    range: r.clone(),
                });
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
        for (n, note) in margin_notes.drain(..).enumerate() {
            sink.push_passage(
                format!("{div_slug}/{k}/margin-{}", n + 1),
                PlannedRole::MarginNote { on: host },
                note,
            );
        }
        current = Some(host);
    }
    sink.close_host(current, &counters);
    if !margin_notes.is_empty() {
        sink.problems.push(format!(
            "{div_slug}: {} margin note(s) with no paragraph after them",
            margin_notes.len()
        ));
    }
}

/// A marginal note as Title Case editions print it: up to three lines, every word capitalised,
/// not all capitals (that's a heading).
fn is_title_case_note(text: &str) -> bool {
    let t = text.trim();
    let words: Vec<&str> = t
        .split_whitespace()
        .map(|w| w.trim_start_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().next().is_some_and(char::is_alphabetic))
        .collect();
    (1..=3).contains(&t.lines().count())
        && !words.is_empty()
        && words
            .iter()
            .all(|w| w.chars().next().is_some_and(char::is_uppercase))
        && t.chars().any(char::is_lowercase)
}

/// The numbers of bracketed footnote markers in a text ("OF PROBABILITY[9]." → [9]).
fn bracket_numbers(bytes: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(open) = rest.iter().position(|&b| b == b'[') {
        rest = &rest[open + 1..];
        let close = rest.iter().position(|&b| b == b']').unwrap_or(0);
        if close > 0 && rest[..close].iter().all(u8::is_ascii_digit) {
            out.extend(
                std::str::from_utf8(&rest[..close])
                    .ok()
                    .and_then(|d| d.parse::<u32>().ok()),
            );
        }
    }
    out
}

/// The number of a footnote paragraph, which starts "[n] " ("[7] Nothing is more useful…").
fn footnote_number(bytes: &[u8]) -> Option<u32> {
    let rest = bytes.strip_prefix(b"[")?;
    let close = rest.iter().position(|&b| b == b']')?;
    let digits = &rest[..close];
    if digits.is_empty()
        || !digits.iter().all(u8::is_ascii_digit)
        || rest
            .get(close + 1)
            .is_some_and(|b| !b.is_ascii_whitespace())
    {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

/// Every line of the paragraph starts with at least four spaces (how Gutenberg prints notes).
fn indented(file: &[u8], r: &Range<usize>) -> bool {
    let line_start = file[..r.start]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let lines = std::iter::once(&file[line_start..r.start]).chain(
        file[r.clone()].split(|&b| b == b'\n').skip(1).map(|l| {
            &l[..l
                .iter()
                .position(|b| !b.is_ascii_whitespace() || *b == b'\r')
                .unwrap_or(l.len())]
        }),
    );
    lines
        .into_iter()
        .all(|indent| indent.iter().filter(|&&b| b == b' ').count() >= 4)
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
    bytes.first() == Some(&b'[')
        && bytes.last() == Some(&b']')
        && bytes[1..bytes.len() - 1]
            .iter()
            .all(|&b| b != b'[' && b != b']')
}

/// "[Note]", "[Note.]", "[Note 1.]", "[Note. 2]": a marker with no content.
fn is_bare_marker(bytes: &[u8]) -> bool {
    bytes.starts_with(b"[Note")
        && bytes.ends_with(b"]")
        && bytes[5..bytes.len() - 1]
            .iter()
            .all(|b| b.is_ascii_digit() || *b == b'.' || *b == b' ')
}

fn count_bare_markers(bytes: &[u8]) -> usize {
    brackets(bytes, 0..bytes.len())
        .into_iter()
        .filter(|r| is_bare_marker(&bytes[r.clone()]))
        .count()
}

/// A division uses numbered sections if its first paragraph starts "1." and at least two
/// paragraphs are numbered.
fn is_numbered(file: &[u8], paras: &[Range<usize>]) -> bool {
    let numbers: Vec<u32> = paras
        .iter()
        .filter_map(|r| section_number(&file[r.clone()]).map(|(n, _)| n))
        .collect();
    numbers.len() >= 2 && numbers[0] == 1 && section_number(&file[paras[0].clone()]).is_some()
}

/// "35. THE…" → (35, length of "35."). Up to 4 digits, then '.', then whitespace.
fn section_number(bytes: &[u8]) -> Option<(u32, usize)> {
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0
        || digits > 4
        || bytes.get(digits) != Some(&b'.')
        || !bytes.get(digits + 1).is_some_and(u8::is_ascii_whitespace)
    {
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
        let Some(dash) = rest.windows(2).position(|w| w == b"--") else {
            break;
        };
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
/// Heading words and the division kind each one opens.
const HEADING_WORDS: [(&str, &str); 6] = [
    ("PART", "part"),
    ("BOOK", "book"),
    ("CHAPTER", "chapter"),
    ("CHAP.", "chapter"),
    ("SECTION", "section"),
    ("SECT.", "section"),
];

/// A line that starts with a heading word and a number, in any case and whatever follows
/// ("CHAP. I. Nature and her laws.", "PART I--Laws of Nature"): its kind and number.
pub(super) fn numbered_entry(line: &str) -> Option<(&'static str, u32)> {
    let n = normalise(line);
    HEADING_WORDS.iter().find_map(|&(word, kind)| {
        let rest = n.strip_prefix(word)?.trim_start();
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        let number = &rest[..end];
        let value = number.parse::<u32>().ok().or_else(|| roman(number))?;
        Some((kind, value))
    })
}

/// A recognised heading: its kind, printed number or word, locator slug and title, if printed.
struct Known {
    kind: String,
    label: String,
    slug: String,
    title: Option<String>,
    /// The title starts on the heading's own line ("CHAPTER I. OF SENSE").
    title_on_heading_line: bool,
}

/// A heading paragraph: a heading word and number on the first line ("PART I.", "CHAP. I."),
/// optionally followed by up to six short lines of title ("Of the PROPRIETY of ACTION."). The
/// title may start on the first line if that line is all capitals ("CHAPTER I. OF SENSE"), so
/// prose ("Part I of this book…") is never a heading. Or a plain heading word alone
/// ("INTRODUCTION", "CONTENTS.", "THE INTRODUCTION").
fn known_heading(text: &str) -> Option<Known> {
    let lines: Vec<&str> = text.trim().lines().map(str::trim).collect();
    if lines.is_empty() || lines.len() > 7 || lines.iter().any(|l| l.len() > 80) {
        return None;
    }
    let first = normalise(lines[0]);
    for (word, kind) in HEADING_WORDS {
        if let Some(rest) = first.strip_prefix(word).and_then(|r| r.strip_prefix(' ')) {
            let (number, same_line) = rest.split_once(' ').unwrap_or((rest, ""));
            let number = number.trim_end_matches('.');
            let value = number.parse::<u32>().ok().or_else(|| roman(number))?;
            if !same_line.is_empty() && lines[0].chars().any(|c| c.is_lowercase()) {
                return None;
            }
            let printed: Vec<&str> = std::iter::once(same_line)
                .chain(lines[1..].iter().copied())
                .filter(|l| !l.is_empty())
                .collect();
            let title = (!printed.is_empty()).then(|| title_text(&printed.join(" ")));
            return Some(Known {
                kind: kind.into(),
                label: number.to_owned(),
                slug: format!("{kind}-{value}"),
                title,
                title_on_heading_line: !same_line.is_empty(),
            });
        }
    }
    let words = [
        "INTRODUCTION",
        "PREFACE",
        "DEDICATION",
        "APPENDIX",
        "CONCLUSION",
        "ADVERTISEMENT",
        "PROLOGUE",
        "EPILOGUE",
        "INDEX",
    ];
    let word = first.trim_end_matches('.');
    let word = word.strip_prefix("THE ").unwrap_or(word);
    (lines.len() == 1).then_some(())?;
    words.iter().find(|w| word == **w).map(|w| {
        let lower = w.to_lowercase();
        Known {
            kind: lower.clone(),
            label: title_case(w),
            slug: lower,
            title: None,
            title_on_heading_line: false,
        }
    })
}

/// A printed title as text: Gutenberg's _italic_ underscores and a final full stop removed.
pub(super) fn title_text(printed: &str) -> String {
    let t = printed.replace('_', "");
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    t.trim_end_matches('.').trim().to_owned()
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
        if digits.get(i + 1).is_some_and(|&next| next > d) {
            total -= d as i64
        } else {
            total += d as i64
        }
    }
    let total = u32::try_from(total).ok().filter(|&t| t > 0)?;
    (to_roman(total) == s).then_some(total)
}

fn to_roman(mut n: u32) -> String {
    let table = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
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
    t.lines().count() == 1
        && t.len() <= 80
        && t.chars().any(|c| c.is_ascii_uppercase())
        && !t.chars().any(|c| c.is_ascii_lowercase())
}

/// "CONTENTS", "CONTENTS.", "TABLE OF CONTENTS", "CONTENTS OF THE CHAPTERS".
fn is_contents_heading(text: &str) -> bool {
    let n = normalise(text);
    let n = n.trim_end_matches('.');
    n == "CONTENTS" || n == "TABLE OF CONTENTS" || n.starts_with("CONTENTS OF ")
}

/// "OF INDIVIDUALITY, AS ONE OF THE ELEMENTS / OF WELL-BEING        103".
fn ends_in_page_number(text: &str) -> bool {
    text.split_whitespace()
        .last()
        .is_some_and(|w| w.parse::<u32>().is_ok())
}

/// Running prose, which a contents list never contains: 20 words or more, not all capitals, not
/// ending in a page number or "ibid.".
fn is_prose(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    let last = words.last().map_or("", |w| w.trim_end_matches('.'));
    words.len() >= 20
        && text.chars().any(|c| c.is_lowercase())
        && !last.eq_ignore_ascii_case("ibid")
        && last.parse::<u32>().is_err()
}

/// A title printed in italics: up to three short lines wholly in Gutenberg's `_…_`
/// ("_Nature and her Laws_.").
fn is_italic_title(text: &str) -> bool {
    let t = text.trim();
    (1..=3).contains(&t.lines().count())
        && t.lines().all(|l| l.trim().len() <= 80)
        && t.starts_with('_')
        && t.trim_end_matches('.').ends_with('_')
}

/// A printer's end mark, no one's words: "END OF PART I.", "FINIS.", "THE END.".
fn is_end_mark(text: &str) -> bool {
    let t = text.trim();
    let n = normalise(t);
    let n = n.trim_end_matches('.');
    t.lines().count() == 1
        && !t.chars().any(char::is_lowercase)
        && (n == "FINIS" || n == "THE END" || (n.starts_with("END OF ") && n.len() <= 60))
}

/// The rest of a wrapped title: up to three short lines, in capitals.
fn is_title_continuation(text: &str) -> bool {
    let t = text.trim();
    (1..=3).contains(&t.lines().count())
        && t.lines().all(|l| l.trim().len() <= 80)
        && t.chars().any(|c| c.is_uppercase())
        && !t.chars().any(|c| c.is_lowercase())
}

/// "* * * * *"
fn is_separator(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && t.chars().all(|c| c == '*' || c == ' ') && t.contains('*')
}

/// Uppercase, whitespace collapsed: for comparing heading lines.
pub(super) fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// "OF THE PRINCIPLES OF HUMAN KNOWLEDGE" → "of-the-principles-of-human-knowledge" (ASCII only).
pub(super) fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for word in text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
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
            c.next().map_or(String::new(), |f| {
                f.to_string() + &c.as_str().to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where a new division goes in the tree. `open` holds the open numbered divisions, outermost
/// first. A heading of a kind already open closes back to that level and becomes its sibling
/// (Section V after Section IV's Parts); a different kind numbered 1 nests inside the innermost
/// open numbered division (Part I after Section IV); a kind seen before inside another kind goes
/// inside the innermost open one of that kind again (Chapter XVII after Part II: chapters are
/// numbered through the Parts); anything else — an unnumbered heading
/// (Introduction, Index) or a new kind not starting at 1 — starts again at the top level, unless
/// it comes straight after a heading with no text of its own: then it opens inside that one
/// (Smith's "SECTION II." then "INTRODUCTION.") and the numbered divisions stay open.
fn nest(
    open: &mut Vec<usize>,
    divisions: &[DivisionNode],
    node: &DivisionNode,
    index: usize,
    under_bare_heading: bool,
) -> Option<usize> {
    let number = heading_number(&node.label);
    if number.is_none()
        && under_bare_heading
        && let Some(&innermost) = open.last()
        && innermost + 1 == index
    {
        return Some(innermost);
    }
    let parent = if let Some(level) = open.iter().position(|&o| divisions[o].kind == node.kind) {
        open.truncate(level);
        open.last().copied()
    } else if number == Some(1) && !open.is_empty() {
        open.last().copied()
    } else if let Some(level) = divisions
        .iter()
        .rev()
        .find(|d| d.kind == node.kind)
        .and_then(|d| d.parent)
        .and_then(|p| {
            open.iter()
                .rposition(|&o| divisions[o].kind == divisions[p].kind)
        })
    {
        open.truncate(level + 1);
        open.last().copied()
    } else {
        open.clear();
        None
    };
    if number.is_some() {
        open.push(index);
    } else {
        open.clear();
    }
    parent
}

/// A heading's number, if its label is one ("4", "IV", "IV.").
fn heading_number(label: &str) -> Option<u32> {
    let l = label.trim().trim_end_matches('.');
    l.parse().ok().or_else(|| roman(l))
}

fn division(kind: &str, label: &str, slug: &str, range: Range<usize>) -> DivisionNode {
    DivisionNode {
        kind: kind.into(),
        label: label.into(),
        slug: slug.into(),
        range,
        parent: None,
        title: None,
    }
}
