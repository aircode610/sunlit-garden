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
    Options, PlannedPassage, PlannedRole, Structure, check_expectations, normalise, numbered_entry,
    slugify, title_text,
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

/// Compile the work inside the element with `id` (a `divN`) of a ThML file.
pub fn compile_thml(file: &[u8], id: &str, options: &Options) -> Structure {
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
    let Some(first) = tokens.iter().position(
        |(_, t)| matches!(t, Token::Start { attrs, .. } if attr(attrs, "id") == Some(id)),
    ) else {
        s.problems.push(format!("no element with id {id:?}"));
        return s;
    };
    let last = matching_end(&tokens, first);
    s.compiled.body = tokens[first].0.start..tokens[last].0.end;

    let mut w = Walker {
        tokens: &tokens,
        src,
        s,
        open: Vec::new(),
        paths: Vec::new(),
        counts: Vec::new(),
    };
    w.skip(&tokens[first].0, "markup");
    w.skip(&tokens[last].0, "markup");
    w.walk(first + 1, last);

    let mut s = w.s;
    let paths = w.paths;
    for (d, path) in paths.iter().enumerate() {
        let has_text = s.passages.iter().any(|p| p.division == d);
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
}

impl Walker<'_> {
    /// Tokens `from..to` (indices), at block level.
    fn walk(&mut self, from: usize, to: usize) {
        let mut i = from;
        while i < to {
            let (span, token) = &self.tokens[i];
            match token {
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
                    } else if name == "p" {
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

    /// A body paragraph: tokens `start` (`<p …>`) to `end` (`</p>`).
    fn paragraph(&mut self, start: usize, end: usize) {
        self.skip(&self.tokens[start].0.clone(), "markup");
        self.skip(&self.tokens[end].0.clone(), "markup");
        let content = trim(self.src, self.tokens[start].0.end..self.tokens[end].0.start);
        if content.is_empty() {
            return;
        }
        let Some(&d) = self.open.last() else {
            self.s
                .problems
                .push("paragraph outside any division".into());
            return;
        };
        self.counts[d] += 1;
        let locator = format!("{}/{}", self.paths[d], self.counts[d]);
        self.s.passages.push(PlannedPassage {
            locator: locator.clone(),
            role: PlannedRole::Text,
            range: content.clone(),
            division: d,
        });
        let host = self.s.passages.len() - 1;

        // Editor's notes inside: nested passages; the author text around them is covered in
        // pieces.
        let mut cursor = content.start;
        let mut notes = 0;
        let mut j = start + 1;
        while j < end {
            if let Token::Start { name, .. } = &self.tokens[j].1
                && name == "note"
            {
                let close = matching_end(self.tokens, j);
                let note = self.tokens[j].0.start..self.tokens[close].0.end;
                self.text_piece(cursor..note.start);
                notes += 1;
                self.unit(note.clone(), UnitKind::Insertion);
                self.s.passages.push(PlannedPassage {
                    locator: format!("{locator}/note-{notes}"),
                    role: PlannedRole::EditorNote { on: host },
                    range: note.clone(),
                    division: d,
                });
                cursor = note.end;
                j = close;
            }
            j += 1;
        }
        self.text_piece(cursor..content.end);
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

/// The text of a markup fragment: tags removed (inline markup never separates words:
/// "un<i>known</i>" is "unknown"), entities decoded, whitespace collapsed.
pub(crate) fn text_of(markup: &str) -> String {
    let mut out = String::new();
    let mut rest = markup;
    while let Some(lt) = rest.find('<') {
        out.push_str(&decode(&rest[..lt], 0).unwrap_or_else(|_| rest[..lt].to_owned()));
        rest = rest[lt..].find('>').map_or("", |gt| &rest[lt + gt + 1..]);
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

/// `range` without surrounding whitespace.
fn trim(src: &str, range: Range<usize>) -> Range<usize> {
    let t = &src[range.clone()];
    let start = range.start + (t.len() - t.trim_start().len());
    let end = range.end - (t.len() - t.trim_end().len());
    start..end.max(start)
}
