//! The argument-unit (ADU) dataset: records, quote resolution and validation.
//!
//! Annotations live in `corpus/annotations/<edition-slug>.jsonl`, one JSON record per line
//! (guidelines: vault `Dataset/ADU Annotation Guidelines`). Units are **authored as exact
//! quotes**; [`resolve`] turns each quote into byte offsets inside its passage's author words,
//! and [`check`] re-verifies everything mechanically, so a hand-computed offset is never trusted.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::corpus::compile::structure::{PlannedRole, Structure};

// ---- Records -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Record {
    Edition(EditionHeader),
    Passage(PassageVerdict),
    Unit(UnitRecord),
    Relation(RelationRecord),
}

/// First line of every file: which edition, how much it covers, and (for dialogues) the cast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditionHeader {
    pub slug: String,
    pub scope: Scope,
    /// Dialogues: every unit's voice must be `speaker:<one of these>`, never `author`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speakers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Only the passages that have a verdict are checked for coverage.
    Pilot,
    /// Every author-text passage of the edition must have a verdict.
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassageVerdict {
    pub locator: String,
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub annotator: Annotator,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Argumentative,
    NonArgumentative,
    NeedsDiscussion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRecord {
    /// `<locator>#u<n>`.
    pub id: String,
    pub locator: String,
    /// Exact words from the passage (whitespace-insensitive).
    pub quote: String,
    /// Byte offsets into the document, filled in by [`resolve`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    #[serde(rename = "type")]
    pub unit_type: UnitType,
    /// `author`, `objector`, `quoted:<who>` or `speaker:<name>`.
    pub voice: String,
    pub stance: Stance,
    pub annotator: Annotator,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitType {
    Claim,
    Premise,
    Objection,
    Reply,
    Example,
    Definition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stance {
    Asserted,
    Entertained,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationRecord {
    /// `<locator>#r<n>`.
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub relation_type: RelationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue: Option<String>,
    pub annotator: Annotator,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationType {
    Support,
    Rebut,
    Undercut,
    Restate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Annotator {
    Agent,
    Amirali,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Gold,
    Rejected,
}

/// Parse a `.jsonl` file. Every bad line is reported (with its line number), not just the first.
pub fn parse_jsonl(text: &str) -> Result<Vec<Record>, Vec<String>> {
    let mut records = Vec::new();
    let mut errors = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Record>(line) {
            Ok(r) => records.push(r),
            Err(e) => errors.push(format!("line {}: {e}", n + 1)),
        }
    }
    if errors.is_empty() { Ok(records) } else { Err(errors) }
}

/// Serialise records back to `.jsonl` (one per line, stable field order).
pub fn to_jsonl(records: &[Record]) -> String {
    let mut out = String::new();
    for r in records {
        out.push_str(&serde_json::to_string(r).expect("records always serialise"));
        out.push('\n');
    }
    out
}

// ---- Where units may go --------------------------------------------------------------------

/// Author-text passages by locator: their range, and the insertion ranges inside them that are
/// **not** the author's words (editor notes/markers) or not annotated (author notes).
struct Hosts {
    by_locator: HashMap<String, (Range<usize>, Vec<Range<usize>>)>,
    /// In reading order, for coverage.
    order: Vec<String>,
}

fn hosts(structure: &Structure) -> Hosts {
    let mut by_locator = HashMap::new();
    let mut order = Vec::new();
    for (i, p) in structure.passages.iter().enumerate() {
        if p.role != PlannedRole::Text {
            continue;
        }
        let excluded: Vec<Range<usize>> = structure
            .passages
            .iter()
            .filter(|q| match q.role {
                PlannedRole::EditorNote { on } | PlannedRole::EditorMarker { in_: on } | PlannedRole::AuthorNote { on } => on == i,
                _ => false,
            })
            .map(|q| q.range.clone())
            .filter(|r| p.range.start <= r.start && r.end <= p.range.end)
            .collect();
        by_locator.insert(p.locator.clone(), (p.range.clone(), excluded));
        order.push(p.locator.clone());
    }
    Hosts { by_locator, order }
}

/// All occurrences of `quote` in `file[range]`, treating any run of whitespace in either as
/// equal to any other run. Returns byte ranges into `file`, trimmed to the quote's words.
fn find_quote(file: &[u8], range: Range<usize>, quote: &str) -> Vec<Range<usize>> {
    // Collapse the haystack's whitespace, remembering where each kept byte came from.
    let mut hay = Vec::new();
    let mut origin = Vec::new();
    let mut in_space = false;
    for (i, &b) in file[range.clone()].iter().enumerate() {
        if b.is_ascii_whitespace() {
            if !in_space {
                hay.push(b' ');
                origin.push(range.start + i);
                in_space = true;
            }
        } else {
            hay.push(b);
            origin.push(range.start + i);
            in_space = false;
        }
    }
    let needle: Vec<u8> = quote.split_whitespace().collect::<Vec<_>>().join(" ").into_bytes();
    if needle.is_empty() || needle.len() > hay.len() {
        return Vec::new();
    }
    (0..=hay.len() - needle.len())
        .filter(|&i| hay[i..i + needle.len()] == needle[..])
        .map(|i| origin[i]..origin[i + needle.len() - 1] + 1)
        .collect()
}

// ---- Resolve -------------------------------------------------------------------------------

/// Fill in `start`/`end` for every unit from its quote. Problems (missing, ambiguous, outside the
/// author's words, unknown locator) are returned; offending units keep `start`/`end` = None.
pub fn resolve(records: &mut [Record], file: &[u8], structure: &Structure) -> Vec<String> {
    let hosts = hosts(structure);
    let mut problems = Vec::new();
    for r in records.iter_mut() {
        let Record::Unit(u) = r else { continue };
        u.start = None;
        u.end = None;
        let Some((range, excluded)) = hosts.by_locator.get(&u.locator) else {
            problems.push(format!("{}: no author-text passage {:?}", u.id, u.locator));
            continue;
        };
        let found = find_quote(file, range.clone(), &u.quote);
        match found.as_slice() {
            [] => problems.push(format!("{}: quote not found in {}: {:?}", u.id, u.locator, u.quote)),
            [one] => {
                if let Some(x) = excluded.iter().find(|x| x.start < one.end && one.start < x.end) {
                    let shown = String::from_utf8_lossy(&file[x.clone()]);
                    problems.push(format!("{}: quote overlaps material that isn't the author's words: {shown:?}", u.id));
                } else {
                    u.start = Some(one.start);
                    u.end = Some(one.end);
                }
            }
            many => problems.push(format!("{}: quote occurs {} times in {} (make it longer)", u.id, many.len(), u.locator)),
        }
    }
    problems
}

// ---- Check ---------------------------------------------------------------------------------

/// What [`check`] found.
#[derive(Debug, Default)]
pub struct Report {
    pub problems: Vec<String>,
    pub passages_with_verdict: usize,
    pub author_passages: usize,
    pub units: usize,
    pub relations: usize,
    pub units_by_type: BTreeMap<String, usize>,
}

/// Every mechanical check from the guidelines (§7). Empty `problems` = valid.
pub fn check(records: &[Record], file: &[u8], structure: &Structure) -> Report {
    let hosts = hosts(structure);
    let mut report = Report { author_passages: hosts.order.len(), ..Report::default() };
    let p = &mut report.problems;

    // Header.
    let header = match records.first() {
        Some(Record::Edition(h)) => Some(h.clone()),
        _ => {
            p.push("first record must be the edition header".into());
            None
        }
    };
    if records.iter().skip(1).any(|r| matches!(r, Record::Edition(_))) {
        p.push("more than one edition header".into());
    }

    // Verdicts: one per passage, known locators.
    let mut verdicts: HashMap<&str, Verdict> = HashMap::new();
    for r in records {
        if let Record::Passage(v) = r {
            if !hosts.by_locator.contains_key(&v.locator) {
                p.push(format!("verdict for unknown author passage {:?}", v.locator));
            }
            if verdicts.insert(&v.locator, v.verdict).is_some() {
                p.push(format!("two verdicts for {}", v.locator));
            }
            if v.verdict == Verdict::NonArgumentative && v.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
                p.push(format!("{}: non-argumentative needs a reason", v.locator));
            }
        }
    }
    report.passages_with_verdict = verdicts.len();
    if header.as_ref().is_some_and(|h| h.scope == Scope::Full) {
        for loc in &hosts.order {
            if !verdicts.contains_key(loc.as_str()) {
                p.push(format!("{loc}: no verdict (scope is full)"));
            }
        }
    }

    // Units.
    let mut ids = HashSet::new();
    let mut units_by_passage: HashMap<&str, Vec<(Range<usize>, &str)>> = HashMap::new();
    let mut unit_work: HashMap<&str, ()> = HashMap::new();
    for r in records {
        let Record::Unit(u) = r else { continue };
        report.units += 1;
        *report.units_by_type.entry(format!("{:?}", u.unit_type).to_lowercase()).or_default() += 1;
        if !ids.insert(u.id.as_str()) {
            p.push(format!("duplicate id {}", u.id));
        }
        if !u.id.starts_with(&format!("{}#u", u.locator)) {
            p.push(format!("{}: unit id must start with \"{}#u\"", u.id, u.locator));
        }
        unit_work.insert(&u.id, ());
        if !verdicts.contains_key(u.locator.as_str()) {
            p.push(format!("{}: its passage {} has no verdict", u.id, u.locator));
        }
        check_voice(u, header.as_ref(), p);
        let Some((range, excluded)) = hosts.by_locator.get(&u.locator) else {
            p.push(format!("{}: no author-text passage {:?}", u.id, u.locator));
            continue;
        };
        let (Some(start), Some(end)) = (u.start, u.end) else {
            p.push(format!("{}: not resolved (run `sunlit annotations resolve`)", u.id));
            continue;
        };
        let span = start..end;
        if start >= end || start < range.start || end > range.end {
            p.push(format!("{}: span {span:?} is outside its passage {range:?}", u.id));
            continue;
        }
        let bytes = &file[span.clone()];
        if bytes.first().is_some_and(u8::is_ascii_whitespace) || bytes.last().is_some_and(u8::is_ascii_whitespace) {
            p.push(format!("{}: span is not trimmed", u.id));
        }
        let text = String::from_utf8_lossy(bytes);
        let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        if squash(&text) != squash(&u.quote) {
            p.push(format!("{}: quote no longer matches the bytes at {span:?}", u.id));
        }
        if excluded.iter().any(|x| x.start < end && start < x.end) {
            p.push(format!("{}: span overlaps material that isn't the author's words", u.id));
        }
        units_by_passage.entry(&u.locator).or_default().push((span, &u.id));
    }

    // No overlaps within a passage; verdicts agree with units.
    for (loc, spans) in &mut units_by_passage {
        spans.sort_by_key(|(r, _)| r.start);
        for w in spans.windows(2) {
            if w[1].0.start < w[0].0.end {
                p.push(format!("{loc}: units {} and {} overlap", w[0].1, w[1].1));
            }
        }
    }
    for (loc, verdict) in &verdicts {
        let n = units_by_passage.get(loc).map_or(0, Vec::len);
        match verdict {
            Verdict::Argumentative if n == 0 => p.push(format!("{loc}: argumentative but has no units")),
            Verdict::NonArgumentative if n > 0 => p.push(format!("{loc}: non-argumentative but has {n} unit(s)")),
            _ => {}
        }
    }

    // Relations.
    for r in records {
        let Record::Relation(rel) = r else { continue };
        report.relations += 1;
        if !ids.insert(rel.id.as_str()) {
            p.push(format!("duplicate id {}", rel.id));
        }
        for end in [&rel.from, &rel.to] {
            if !unit_work.contains_key(end.as_str()) {
                p.push(format!("{}: endpoint {end} is not a unit", rel.id));
            }
        }
        if rel.from == rel.to {
            p.push(format!("{}: relation from a unit to itself", rel.id));
        }
    }
    report
}

fn check_voice(u: &UnitRecord, header: Option<&EditionHeader>, p: &mut Vec<String>) {
    let speakers = header.map(|h| h.speakers.as_slice()).unwrap_or_default();
    let valid_name = |s: &str| !s.is_empty() && s.split('-').all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
    let ok = if !speakers.is_empty() {
        // A dialogue: only its cast may speak.
        u.voice.strip_prefix("speaker:").is_some_and(|name| speakers.iter().any(|s| s == name))
    } else {
        u.voice == "author"
            || u.voice == "objector"
            || u.voice.strip_prefix("quoted:").is_some_and(valid_name)
            || u.voice.strip_prefix("speaker:").is_some_and(valid_name)
    };
    if !ok {
        let allowed = if speakers.is_empty() {
            "author | objector | quoted:<who> | speaker:<name>".to_string()
        } else {
            format!("speaker:<{}>", speakers.join("|"))
        };
        p.push(format!("{}: voice {:?} not allowed here ({allowed})", u.id, u.voice));
    }
}
