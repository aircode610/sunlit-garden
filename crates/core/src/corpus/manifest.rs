//! The hand-curated source list, `corpus/sources.toml`:
//!
//! ```toml
//! [[thinker]]            # a person
//! [[work]]               # an abstract work, by `thinker = "<slug>"`
//! [[document]]           # one downloadable file, with its pinned `sha256`
//! [[document.edition]]   # one work inside that file, `work = "<thinker>/<work>"`
//! [[mirror]]             # where a provider's files are downloaded from instead
//! ```
//!
//! Parsing happens in two stages: serde reads the raw shape (rejecting unknown keys, so a
//! typo can't be silently ignored), then `validate` checks every value and reference and
//! reports **all** problems at once. Nothing is imported from a list that doesn't validate.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use serde::Deserialize;

use super::compile::structure::{
    Attribution, Expect, HeadingOverride, InsertionRule, MarginNotes, Options, PassageUnit,
};
use crate::values::{Sha256, Slug, WikidataId};

/// A validated source list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub thinkers: Vec<ThinkerEntry>,
    pub works: Vec<WorkEntry>,
    pub documents: Vec<DocumentEntry>,
    pub mirrors: Vec<MirrorEntry>,
}

/// Download a provider's files from `url` instead of `origin` (e.g. Gutenberg's official
/// mirror). Documents keep their canonical URL as provenance; only the download is redirected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorEntry {
    pub provider: Slug,
    /// Canonical URL prefix, ending in `/`: `https://www.gutenberg.org/`.
    pub origin: String,
    /// Mirror URL prefix, ending in `/`: `https://gutenberg.pglaf.org/`.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkerEntry {
    pub slug: Slug,
    pub name: String,
    pub wikidata_id: Option<WikidataId>,
    pub born_year: Option<i16>,
    pub died_year: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkEntry {
    /// Slug of the thinker who wrote it (must be listed).
    pub thinker: Slug,
    pub slug: Slug,
    pub title: String,
    pub wikidata_id: Option<WikidataId>,
    pub first_year: Option<i16>,
    pub original_language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentEntry {
    pub provider: Slug,
    pub provider_id: String,
    /// Exact file URL (HTTPS). Its bytes must hash to `sha256`.
    pub url: String,
    pub licence: String,
    pub media_type: String,
    /// The pinned hash. `None` = not yet pinned: the importer refuses and prints what it saw.
    pub sha256: Option<Sha256>,
    pub editions: Vec<EditionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditionEntry {
    /// The work, as `(thinker slug, work slug)`.
    pub work: (Slug, Slug),
    pub slug: Slug,
    pub title: String,
    pub language: String,
    pub translator: Option<String>,
    pub published_year: Option<i16>,
    pub range: RangeSpec,
    /// How the compiler should read this edition: heading overrides, who wrote its bracketed
    /// insertions, and facts to check.
    pub compile: Options,
}

/// Where an edition's text sits inside its document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeSpec {
    /// Between Project Gutenberg's START/END markers (the default for Gutenberg files).
    GutenbergBody,
    /// Explicit byte offsets, e.g. for one work inside a multi-work volume.
    Bytes { start: i64, end: i64 },
    /// One element of an XML file, by its `id` (a ThML `divN`: one work in a CCEL volume).
    Element { id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// Not valid TOML, or the wrong shape (missing or unknown keys).
    Toml(String),
    /// Well-formed, but these values or references are wrong.
    Invalid(Vec<String>),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Toml(msg) => write!(f, "source list is not well-formed: {msg}"),
            ManifestError::Invalid(errors) => {
                writeln!(f, "source list has {} problem(s):", errors.len())?;
                for e in errors {
                    writeln!(f, "  - {e}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    pub fn parse(toml_text: &str) -> Result<Self, ManifestError> {
        let raw: RawManifest =
            toml::from_str(toml_text).map_err(|e| ManifestError::Toml(e.to_string()))?;
        validate(raw)
    }

    /// Where to download `document` from: its URL with a mirror's origin swapped for the
    /// mirror, or the URL itself if no mirror covers it.
    pub fn fetch_url(&self, document: &DocumentEntry) -> String {
        self.mirrored(document.provider.as_str(), &document.url)
    }

    /// Where to download any `url` of `provider` (a document, or e.g. its catalogue).
    pub fn mirrored(&self, provider: &str, url: &str) -> String {
        self.mirrors
            .iter()
            .find_map(|m| {
                let path = url.strip_prefix(&m.origin)?;
                (m.provider.as_str() == provider).then(|| format!("{}{path}", m.url))
            })
            .unwrap_or_else(|| url.to_owned())
    }
}

// ---- Stage 1: the raw TOML shape --------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    #[serde(default)]
    thinker: Vec<RawThinker>,
    #[serde(default)]
    work: Vec<RawWork>,
    #[serde(default)]
    document: Vec<RawDocument>,
    #[serde(default)]
    mirror: Vec<RawMirror>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMirror {
    provider: String,
    origin: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawThinker {
    slug: String,
    name: String,
    wikidata: Option<String>,
    born: Option<i16>,
    died: Option<i16>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWork {
    thinker: String,
    slug: String,
    title: String,
    wikidata: Option<String>,
    first_year: Option<i16>,
    original_language: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    provider: String,
    provider_id: String,
    url: String,
    licence: String,
    media_type: String,
    sha256: Option<String>,
    #[serde(default)]
    edition: Vec<RawEdition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEdition {
    work: String,
    slug: String,
    title: String,
    language: String,
    translator: Option<String>,
    published_year: Option<i16>,
    range: Option<RawRange>,
    /// `passages = "paragraphs"` / `"numbered-sections"`; default: decided from the text.
    #[serde(default)]
    passages: RawPassages,
    /// `margin_notes = "title-case"` if the edition prints the author's marginal notes as
    /// Title Case paragraphs; default: none.
    #[serde(default)]
    margin_notes: RawMarginNotes,
    #[serde(default)]
    heading: Vec<RawHeading>,
    #[serde(default)]
    insertion: Vec<RawInsertion>,
    expect: Option<RawExpect>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHeading {
    text: String,
    kind: String,
    label: String,
    slug: String,
    /// `by = "editor"`: the division is the editor's (a modern introduction), not passages.
    by: Option<RawBy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInsertion {
    prefix: String,
    by: RawBy,
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
enum RawMarginNotes {
    #[default]
    None,
    TitleCase,
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
enum RawPassages {
    #[default]
    Auto,
    Paragraphs,
    NumberedSections,
}

/// `by = "editor"` or `by = "author"`; anything else fails to parse.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum RawBy {
    Editor,
    Author,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExpect {
    divisions: Option<Vec<String>>,
    #[serde(default)]
    text_passages: BTreeMap<String, usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRange {
    start: Option<i64>,
    end: Option<i64>,
    /// `range = { div = "iii" }`: the XML element with this id.
    div: Option<String>,
}

// ---- Stage 2: validation ----------------------------------------------------------------

/// Collects problems instead of stopping at the first one.
#[derive(Default)]
struct Problems(Vec<String>);

impl Problems {
    /// Run a parse; on failure record "<context>: <error>" and return None.
    fn check<T, E: fmt::Display>(&mut self, context: &str, result: Result<T, E>) -> Option<T> {
        result
            .map_err(|e| self.0.push(format!("{context}: {e}")))
            .ok()
    }

    fn push(&mut self, problem: String) {
        self.0.push(problem);
    }
}

fn validate(raw: RawManifest) -> Result<Manifest, ManifestError> {
    let mut problems = Problems::default();

    // Thinkers.
    let mut thinker_slugs = HashSet::new();
    let mut thinkers = Vec::new();
    for t in raw.thinker {
        let ctx = format!("thinker {:?}", t.slug);
        let slug = problems.check(&ctx, Slug::parse(&t.slug));
        let wikidata_id = t
            .wikidata
            .map(|q| problems.check(&ctx, WikidataId::parse(&q)));
        if let Some(slug) = &slug
            && !thinker_slugs.insert(slug.clone())
        {
            problems.push(format!("{ctx}: listed twice"));
        }
        if let (Some(slug), wikidata_id) = (slug, wikidata_id) {
            thinkers.push(ThinkerEntry {
                slug,
                name: t.name,
                wikidata_id: wikidata_id.flatten(),
                born_year: t.born,
                died_year: t.died,
            });
        }
    }

    // Works: (thinker, slug) unique, thinker must be listed.
    let mut work_keys = HashSet::new();
    let mut works = Vec::new();
    for w in raw.work {
        let ctx = format!("work {:?}/{:?}", w.thinker, w.slug);
        let thinker = problems.check(&ctx, Slug::parse(&w.thinker));
        let slug = problems.check(&ctx, Slug::parse(&w.slug));
        let wikidata_id = w
            .wikidata
            .map(|q| problems.check(&ctx, WikidataId::parse(&q)));
        if let Some(t) = &thinker
            && !thinker_slugs.contains(t)
        {
            problems.push(format!("{ctx}: thinker {:?} is not listed", w.thinker));
        }
        if let (Some(thinker), Some(slug)) = (thinker, slug) {
            if !work_keys.insert((thinker.clone(), slug.clone())) {
                problems.push(format!("{ctx}: listed twice"));
            }
            works.push(WorkEntry {
                thinker,
                slug,
                title: w.title,
                wikidata_id: wikidata_id.flatten(),
                first_year: w.first_year,
                original_language: w.original_language,
            });
        }
    }

    // Documents and their editions.
    let mut document_keys = HashSet::new();
    let mut edition_keys = HashSet::new();
    let mut documents = Vec::new();
    for d in raw.document {
        let ctx = format!("document {:?} {:?}", d.provider, d.provider_id);
        let provider = problems.check(&ctx, Slug::parse(&d.provider));
        let sha256 = d.sha256.map(|h| problems.check(&ctx, Sha256::from_hex(&h)));
        if !d.url.starts_with("https://") {
            problems.push(format!("{ctx}: url must be https:// (got {:?})", d.url));
        }
        if !document_keys.insert((d.provider.clone(), d.provider_id.clone())) {
            problems.push(format!("{ctx}: listed twice"));
        }
        if d.edition.is_empty() {
            problems.push(format!("{ctx}: has no [[document.edition]]"));
        }

        let mut editions = Vec::new();
        for e in d.edition {
            let ctx = format!("{ctx}, edition {:?} of {:?}", e.slug, e.work);
            let slug = problems.check(&ctx, Slug::parse(&e.slug));
            let work = parse_work_ref(&e.work, &mut problems, &ctx);
            if let Some(w) = &work
                && !work_keys.contains(w)
            {
                problems.push(format!("{ctx}: work {:?} is not listed", e.work));
            }
            let range = match (e.range, d.provider.as_str()) {
                (
                    Some(RawRange {
                        start: Some(start),
                        end: Some(end),
                        div: None,
                    }),
                    _,
                ) if start >= 0 && start < end => Some(RangeSpec::Bytes { start, end }),
                (
                    Some(RawRange {
                        start: None,
                        end: None,
                        div: Some(id),
                    }),
                    _,
                ) if !id.is_empty() => Some(RangeSpec::Element { id }),
                (Some(r), _) => {
                    problems.push(format!(
                        "{ctx}: range needs either 0 <= start < end or div = \"<id>\" (got {:?}..{:?}, div {:?})",
                        r.start, r.end, r.div
                    ));
                    None
                }
                (None, "gutenberg") => Some(RangeSpec::GutenbergBody),
                (None, _) => {
                    problems.push(format!("{ctx}: needs an explicit range = {{ start, end }} (only gutenberg has a default)"));
                    None
                }
            };
            let compile = Options {
                margin_notes: match e.margin_notes {
                    RawMarginNotes::None => MarginNotes::None,
                    RawMarginNotes::TitleCase => MarginNotes::TitleCase,
                },
                passages: match e.passages {
                    RawPassages::Auto => PassageUnit::Auto,
                    RawPassages::Paragraphs => PassageUnit::Paragraphs,
                    RawPassages::NumberedSections => PassageUnit::NumberedSections,
                },
                headings: e
                    .heading
                    .into_iter()
                    .map(|h| {
                        if Slug::parse(&h.slug).is_err() {
                            problems.push(format!(
                                "{ctx}: heading slug {:?} is not a valid slug",
                                h.slug
                            ));
                        }
                        HeadingOverride {
                            text: h.text,
                            kind: h.kind,
                            label: h.label,
                            slug: h.slug,
                            by: h.by.map(|b| match b {
                                RawBy::Editor => Attribution::Editor,
                                RawBy::Author => Attribution::Author,
                            }),
                        }
                    })
                    .collect(),
                insertions: e
                    .insertion
                    .into_iter()
                    .map(|i| InsertionRule {
                        prefix: i.prefix,
                        by: match i.by {
                            RawBy::Editor => Attribution::Editor,
                            RawBy::Author => Attribution::Author,
                        },
                    })
                    .collect(),
                expect: e.expect.map(|x| Expect {
                    divisions: x.divisions,
                    text_passages: x.text_passages,
                }),
            };
            if let (Some(work), Some(slug), Some(range)) = (work, slug, range) {
                if !edition_keys.insert((work.clone(), slug.clone())) {
                    problems.push(format!("{ctx}: listed twice"));
                }
                editions.push(EditionEntry {
                    work,
                    slug,
                    title: e.title,
                    language: e.language,
                    translator: e.translator,
                    published_year: e.published_year,
                    range,
                    compile,
                });
            }
        }

        if let (Some(provider), sha256) = (provider, sha256) {
            documents.push(DocumentEntry {
                provider,
                provider_id: d.provider_id,
                url: d.url,
                licence: d.licence,
                media_type: d.media_type,
                sha256: sha256.flatten(),
                editions,
            });
        }
    }

    // Mirrors.
    let mut mirrors: Vec<MirrorEntry> = Vec::new();
    for m in raw.mirror {
        let ctx = format!("mirror for {:?}", m.provider);
        // The trailing slash matters: "https://www.gutenberg.org" would also be a prefix of
        // "https://www.gutenberg.org.evil.example/".
        for (field, prefix) in [("origin", &m.origin), ("url", &m.url)] {
            if !(prefix.starts_with("https://") && prefix.ends_with('/')) {
                problems.push(format!(
                    "{ctx}: {field} must start with https:// and end with / (got {prefix:?})"
                ));
            }
        }
        if mirrors.iter().any(|x| x.provider.as_str() == m.provider) {
            problems.push(format!("{ctx}: listed twice"));
        }
        if let Some(provider) = problems.check(&ctx, Slug::parse(&m.provider)) {
            mirrors.push(MirrorEntry {
                provider,
                origin: m.origin,
                url: m.url,
            });
        }
    }
    // A provider with a mirror is only ever downloaded through it.
    for d in &documents {
        for m in mirrors.iter().filter(|m| m.provider == d.provider) {
            if !d.url.starts_with(&m.origin) {
                problems.push(format!(
                    "document {:?} {:?}: url {:?} is not under the mirror origin {:?}",
                    d.provider.as_str(),
                    d.provider_id,
                    d.url,
                    m.origin
                ));
            }
        }
    }

    if problems.0.is_empty() {
        Ok(Manifest {
            thinkers,
            works,
            documents,
            mirrors,
        })
    } else {
        Err(ManifestError::Invalid(problems.0))
    }
}

/// `"david-hume/dialogues-concerning-natural-religion"` → (thinker slug, work slug).
fn parse_work_ref(reference: &str, problems: &mut Problems, ctx: &str) -> Option<(Slug, Slug)> {
    let Some((thinker, work)) = reference.split_once('/') else {
        problems.push(format!("{ctx}: work must be \"<thinker>/<work>\""));
        return None;
    };
    let thinker = problems.check(ctx, Slug::parse(thinker))?;
    let work = problems.check(ctx, Slug::parse(work))?;
    Some((thinker, work))
}
