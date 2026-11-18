//! Front-end for ThML, the XML of the Christian Classics Ethereal Library (CCEL).
//!
//! The work is one element of the file (a `divN`, chosen by its `id`). Inside it, `divN`
//! elements are divisions (kind and number from their `title`), headings and the chapter
//! summary (`<p class="SectionInfo">`) are heading material, and every other `<p>` is a passage
//! whose byte range is the element's content in the untouched file, markup included. `<note>`s
//! inside a paragraph are the editor's. Markup between paragraphs is covered as skipped units, so
//! the coverage check still proves every byte is accounted for: text outside any known element
//! is reported, never silently dropped.

use std::ops::Range;

use super::structure::{
    Options, PassageUnit, PlannedPassage, PlannedRole, Structure, check_expectations, normalise,
    numbered_entry, slugify, title_text,
};
use super::{Compiled, DivisionNode, Unit, UnitKind, check_coverage};
use crate::corpus::xhtml::{Token, decode, tokenize_spans};

/// Words that name an unnumbered division when they are a `divN`'s whole title.
const DIVISION_WORDS: [&str; 9] = [
    "PREFACE",
    "INTRODUCTION",
    "DEDICATION",
    "APPENDIX",
    "CONCLUSION",
    "ADVERTISEMENT",
    "PROLOGUE",
    "EPILOGUE",
    "INDEX",
];

/// Compile the work inside the element with `id` (a `divN`) of a ThML file, leaving out the
/// elements in `except` (another work nested inside it).
pub fn compile_thml(file: &[u8], id: &str, except: &[String], options: &Options) -> Structure {
    let mut s = Structure {
        compiled: Compiled {
            body: 0..0,
            divisions: Vec::new(),
            units: Vec::new(),
        },
        passages: Vec::new(),
        toc: Vec::new(),
        problems: Vec::new(),
        warnings: Vec::new(),
        markup: true,
    };
    let Ok(src) = std::str::from_utf8(file) else {
        s.problems.push("file is not UTF-8".into());
        return s;
    };
    let tokens = match tokenize_spans(src) {
        Ok(t) => t,
        Err(e) => {
            s.problems.push(format!("not well-formed: {e}"));
            return s;
        }
    };
    let Some((first, last)) = element(&tokens, id) else {
        s.problems.push(format!("no element with id {id:?}"));
        return s;
    };
    s.compiled.body = tokens[first].0.start..tokens[last].0.end;

    let mut w = Walker {
        tokens: &tokens,
        src,
        s,
        open: Vec::new(),
        paths: Vec::new(),
        counts: Vec::new(),
        work_count: 0,
        numbered: options.passages == PassageUnit::NumberedSections,
        notes: Vec::new(),
        except,
    };
    w.skip(&tokens[first].0, "markup");
    w.skip(&tokens[last].0, "markup");
    w.walk(first + 1, last);

    let mut s = w.s;
    let paths = w.paths;
    for (d, path) in paths.iter().enumerate() {
        let has_text = s.passages.iter().any(|p| p.division == Some(d));
        let has_children = s.compiled.divisions.iter().any(|x| x.parent == Some(d));
        if !has_text && !has_children {
            s.problems.push(format!("division {path:?} has no text"));
        }
    }
    let mut problems = std::mem::take(&mut s.problems);
    check_expectations(options, &paths, &s.passages, &mut problems);
    s.compiled.units.sort_by_key(|u| u.range.start);
    problems.extend(
        check_coverage(file, &s.compiled)
            .iter()
            .map(|p| p.to_string()),
    );
    s.problems = problems;
    s
}

struct Walker<'a> {
    tokens: &'a [(Range<usize>, Token)],
    src: &'a str,
    s: Structure,
    /// Open divisions, innermost last.
    open: Vec<usize>,
    /// Each division's full locator path, parallel to `s.compiled.divisions`.
    paths: Vec<String>,
    /// Text passages so far in each division (their numbers).
    counts: Vec<usize>,
    /// Text passages so far directly in the work (an undivided work).
    work_count: usize,
    /// Declared `passages = "numbered-sections"`: "1. …" starts a passage, other paragraphs
    /// continue it.
    numbered: bool,
    /// Editor's notes so far on each passage (by passage index).
    notes: Vec<usize>,
    /// Ids of elements that are another work: covered, never walked into.
    except: &'a [String],
}

impl Walker<'_> {
    /// Tokens `from..to` (indices), at block level.
    fn walk(&mut self, from: usize, to: usize) {
        let mut i = from;
        while i < to {
            let (span, token) = &self.tokens[i];
            match token {
                Token::Start { attrs, .. }
                    if attr(attrs, "id").is_some_and(|x| self.except.iter().any(|e| e == x)) =>
                {
                    let end = matching_end(self.tokens, i);
                    self.unit(
                        span.start..self.tokens[end].0.end,
                        UnitKind::Skipped("another work"),
                    );
                    i = end + 1;
                    continue;
                }
                Token::Start { name, attrs } if is_div(name) => {
                    self.open_division(attrs, span.start);
                    self.skip(span, "markup");
                }
                Token::End { name } if is_div(name) => {
                    if let Some(d) = self.open.pop() {
                        self.s.compiled.divisions[d].range.end = span.end;
                    }
                    self.skip(span, "markup");
                }
                Token::Start { name, attrs } => {
                    let end = matching_end(self.tokens, i);
                    let whole = span.start..self.tokens[end].0.end;
                    if is_heading(name) {
                        self.unit(whole, UnitKind::DivisionHeading);
                    } else if name == "p" && attr(attrs, "class") == Some("SectionInfo") {
                        self.unit(whole.clone(), UnitKind::DivisionHeading);
                        let title = title_text(&text_of(&self.src[whole]));
                        if let Some(&d) = self.open.last() {
                            self.s.compiled.divisions[d].title = Some(title);
                        }
                    } else if name == "p"
                        && attr(attrs, "style").is_some_and(|st| st.contains("text-align:center"))
                        && !self.has_text(self.open.last().copied())
                    {
                        // Centred lines before the text: the work's printed title block.
                        self.unit(whole, UnitKind::DivisionHeading);
                    } else if name == "p" || name == "verse" {
                        // Verse quoted in the text is a passage like a paragraph.
                        self.paragraph(i, end);
                    } else if name == "pb" {
                        self.skip(span, "page break");
                    } else {
                        // Any other block element (blockquote, …): its tags are markup, its
                        // content is walked like the division's.
                        self.skip(span, "markup");
                        self.skip(&self.tokens[end].0.clone(), "markup");
                        self.walk(i + 1, end);
                    }
                    i = end + 1;
                    continue;
                }
                // Text at block level: whitespace is fine; anything else stays uncovered and the
                // coverage check reports it.
                Token::Text(_) | Token::End { .. } => {}
            }
            i += 1;
        }
    }

    fn open_division(&mut self, attrs: &[(String, String)], start: usize) {
        let title = attr(attrs, "title").unwrap_or_default();
        let (kind, label, slug) = division_name(title);
        let parent = self.open.last().copied();
        let path = match parent {
            Some(p) => format!("{}/{slug}", self.paths[p]),
            None => slug.clone(),
        };
        if self.paths.contains(&path) {
            self.s
                .problems
                .push(format!("two divisions with locator {path:?}"));
        }
        self.s.compiled.divisions.push(DivisionNode {
            kind,
            label,
            slug,
            range: start..start,
            parent,
            title: None,
        });
        self.paths.push(path);
        self.counts.push(0);
        self.open.push(self.s.compiled.divisions.len() - 1);
    }

    /// Whether the division (or, for `None`, the work itself) has text passages yet.
    fn has_text(&self, container: Option<usize>) -> bool {
        self.s
            .passages
            .iter()
            .any(|p| p.division == container && p.role == PlannedRole::Text)
    }

    /// A body paragraph: tokens `start` (`<p …>`) to `end` (`</p>`).
    fn paragraph(&mut self, start: usize, end: usize) {
        self.skip(&self.tokens[start].0.clone(), "markup");
        self.skip(&self.tokens[end].0.clone(), "markup");
        let mut content = trim(self.src, self.tokens[start].0.end..self.tokens[end].0.start);
        if content.is_empty() {
            return;
        }
        // Only markup and whitespace (a page break, a no-break space): no passage.
        if text_of(&self.src[content.clone()]).is_empty() {
            self.unit(content, UnitKind::Skipped("markup"));
            return;
        }
        let container = self.open.last().copied();
        let prefix = container.map_or(String::new(), |d| format!("{}/", self.paths[d]));
        let host = if self.numbered {
            match section_number(&self.src[content.clone()]) {
                Some((number, len)) => {
                    // The printed number is a label, not the text.
                    self.unit(content.start..content.start + len, UnitKind::SectionNumber);
                    content = trim(self.src, content.start + len..content.end);
                    self.new_passage(format!("{prefix}{number}"), container, content.clone())
                }
                None => match self
                    .s
                    .passages
                    .iter()
                    .rposition(|p| p.division == container && p.role == PlannedRole::Text)
                {
                    // A continuation paragraph: the section runs on over it.
                    Some(h) => {
                        self.s.passages[h].range.end = content.end;
                        h
                    }
                    None => {
                        self.s
                            .problems
                            .push(format!("{prefix}: text before the first numbered section"));
                        self.unit(content, UnitKind::Text);
                        return;
                    }
                },
            }
        } else {
            let n = match container {
                Some(d) => {
                    self.counts[d] += 1;
                    self.counts[d]
                }
                None => {
                    self.work_count += 1;
                    self.work_count
                }
            };
            self.new_passage(format!("{prefix}{n}"), container, content.clone())
        };
        let locator = self.s.passages[host].locator.clone();

        // Editor's notes inside: nested passages; the author text around them is covered in
        // pieces.
        let mut cursor = content.start;
        let mut j = start + 1;
        while j < end {
            if let Token::Start { name, .. } = &self.tokens[j].1
                && name == "note"
            {
                let close = matching_end(self.tokens, j);
                let note = self.tokens[j].0.start..self.tokens[close].0.end;
                self.text_piece(cursor..note.start);
                self.notes[host] += 1;
                self.unit(note.clone(), UnitKind::Insertion);
                self.s.passages.push(PlannedPassage {
                    locator: format!("{locator}/note-{}", self.notes[host]),
                    role: PlannedRole::EditorNote { on: host },
                    range: note.clone(),
                    division: container,
                });
                self.notes.push(0);
                cursor = note.end;
                j = close;
            }
            j += 1;
        }
        self.text_piece(cursor..content.end);
    }

    fn new_passage(
        &mut self,
        locator: String,
        division: Option<usize>,
        range: Range<usize>,
    ) -> usize {
        self.s.passages.push(PlannedPassage {
            locator,
            role: PlannedRole::Text,
            range,
            division,
        });
        self.notes.push(0);
        self.s.passages.len() - 1
    }

    fn text_piece(&mut self, range: Range<usize>) {
        let r = trim(self.src, range);
        if !r.is_empty() {
            self.unit(r, UnitKind::Text);
        }
    }

    fn skip(&mut self, span: &Range<usize>, why: &'static str) {
        // A self-closing tag's start and end share one span: cover it once.
        if self
            .s
            .compiled
            .units
            .last()
            .is_some_and(|u| u.range == *span)
        {
            return;
        }
        self.unit(span.clone(), UnitKind::Skipped(why));
    }

    fn unit(&mut self, range: Range<usize>, kind: UnitKind) {
        self.s.compiled.units.push(Unit { kind, range });
    }
}

/// "Chapter II." → (chapter, II, chapter-2); "Preface" → (preface, Preface, preface); anything
/// else is a generic division named by its title.
fn division_name(title: &str) -> (String, String, String) {
    let t = title.trim().trim_end_matches('.');
    if let Some((kind, n)) = numbered_entry(t) {
        let label = t.split_whitespace().nth(1).unwrap_or_default();
        let label = label.trim_end_matches('.').to_owned();
        return (kind.to_owned(), label, format!("{kind}-{n}"));
    }
    let n = normalise(t);
    if let Some(w) = DIVISION_WORDS.iter().find(|w| n == **w) {
        let lower = w.to_lowercase();
        return (lower.clone(), t.to_owned(), lower);
    }
    ("division".to_owned(), t.to_owned(), slugify(&n))
}

/// "1. IF one doubts…" → (1, length of "1."): a section number printed at the start.
fn section_number(text: &str) -> Option<(u32, usize)> {
    let digits = text.find(|c: char| !c.is_ascii_digit())?;
    let n: u32 = text[..digits].parse().ok()?;
    text[digits..].starts_with(". ").then_some((n, digits + 1))
}

/// The text of a markup fragment: tags removed, entities decoded, whitespace collapsed. Inline
/// markup never separates words ("un<i>known</i>" is "unknown"); a verse line or line break
/// does.
pub(crate) fn text_of(markup: &str) -> String {
    let mut out = String::new();
    let mut rest = markup;
    while let Some(lt) = rest.find('<') {
        out.push_str(&decode(&rest[..lt], 0).unwrap_or_else(|_| rest[..lt].to_owned()));
        let Some(gt) = rest[lt..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[lt + 1..lt + gt].trim_start_matches('/');
        let name = tag
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        if matches!(name, "l" | "br" | "p" | "verse") {
            out.push(' ');
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(&decode(rest, 0).unwrap_or_else(|_| rest.to_owned()));
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn is_div(name: &str) -> bool {
    name.len() == 4 && name.starts_with("div") && name.as_bytes()[3].is_ascii_digit()
}

fn is_heading(name: &str) -> bool {
    name.len() == 2 && name.starts_with('h') && name.as_bytes()[1].is_ascii_digit()
}

/// The start and end token indices of the element with `id`.
fn element(tokens: &[(Range<usize>, Token)], id: &str) -> Option<(usize, usize)> {
    let first = tokens.iter().position(
        |(_, t)| matches!(t, Token::Start { attrs, .. } if attr(attrs, "id") == Some(id)),
    )?;
    Some((first, matching_end(tokens, first)))
}

/// The bytes of the element with `id`, from its start tag to its end tag: an edition's range
/// inside a multi-work XML file.
pub fn element_range(file: &[u8], id: &str) -> Result<Range<usize>, String> {
    let src = std::str::from_utf8(file).map_err(|_| "file is not UTF-8".to_owned())?;
    let tokens = tokenize_spans(src).map_err(|e| format!("not well-formed: {e}"))?;
    let (first, last) = element(&tokens, id).ok_or_else(|| format!("no element with id {id:?}"))?;
    Ok(tokens[first].0.start..tokens[last].0.end)
}

/// The index of the end token closing the start token at `start`.
fn matching_end(tokens: &[(Range<usize>, Token)], start: usize) -> usize {
    let mut depth = 0usize;
    for (k, (_, t)) in tokens.iter().enumerate().skip(start) {
        match t {
            Token::Start { .. } => depth += 1,
            Token::End { .. } => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            Token::Text(_) => {}
        }
    }
    tokens.len() - 1
}

/// `range` without surrounding whitespace (empty if there's nothing but whitespace).
fn trim(src: &str, range: Range<usize>) -> Range<usize> {
    // A self-closing element (`<p/>`) has no content: its "inside" runs backwards.
    if range.start >= range.end {
        return range.start..range.start;
    }
    let t = &src[range.clone()];
    let start = range.start + (t.len() - t.trim_start().len());
    let end = range.end - (t.len() - t.trim_end().len());
    start..end.max(start)
}
