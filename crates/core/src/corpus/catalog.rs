//! Project Gutenberg's catalogue, `pg_catalog.csv`: one row per book with its id, title,
//! languages, authors, subjects and Library of Congress class. Used by `sunlit discover` to
//! **suggest** books; nothing here is imported or trusted as evidence (the catalogue changes
//! daily — only pinned files are evidence).

use std::fmt;

use serde::Deserialize;

/// One book in the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The Gutenberg ebook number (`provider_id` in the source list).
    pub id: u32,
    /// `Text`, `Sound`, `Image`, `Dataset`, …
    pub kind: String,
    /// Date Gutenberg released it (`YYYY-MM-DD`).
    pub issued: String,
    /// As catalogued; may span several lines (title + subtitle).
    pub title: String,
    pub languages: Vec<String>,
    /// "Hume, David, 1711-1776", with contributors' roles in brackets ("… [Editor]").
    pub authors: Vec<String>,
    pub subjects: Vec<String>,
    /// Library of Congress classes: `B` (philosophy), `BL` (religion), `BX`, …
    pub locc: Vec<String>,
    pub bookshelves: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CatalogError {
    /// The header isn't the one we know: Gutenberg changed the format, so refuse to guess.
    UnexpectedHeader(Vec<String>),
    /// A row couldn't be read (with its line number).
    Row { line: u64, error: String },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::UnexpectedHeader(h) => {
                write!(f, "catalogue header changed: {h:?} (expected {HEADER:?})")
            }
            CatalogError::Row { line, error } => write!(f, "catalogue line {line}: {error}"),
        }
    }
}

impl std::error::Error for CatalogError {}

const HEADER: [&str; 9] = [
    "Text#",
    "Type",
    "Issued",
    "Title",
    "Language",
    "Authors",
    "Subjects",
    "LoCC",
    "Bookshelves",
];

/// A row exactly as the CSV has it.
#[derive(Deserialize)]
struct RawRow {
    #[serde(rename = "Text#")]
    id: u32,
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Issued")]
    issued: String,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Language")]
    language: String,
    #[serde(rename = "Authors")]
    authors: String,
    #[serde(rename = "Subjects")]
    subjects: String,
    #[serde(rename = "LoCC")]
    locc: String,
    #[serde(rename = "Bookshelves")]
    bookshelves: String,
}

/// Parse the whole catalogue (uncompressed CSV bytes).
pub fn parse(csv_bytes: &[u8]) -> Result<Vec<CatalogEntry>, CatalogError> {
    let mut reader = csv::Reader::from_reader(csv_bytes);
    let header: Vec<String> = reader
        .headers()
        .map_err(|e| CatalogError::Row {
            line: 1,
            error: e.to_string(),
        })?
        .iter()
        .map(str::to_owned)
        .collect();
    if header != HEADER {
        return Err(CatalogError::UnexpectedHeader(header));
    }
    reader
        .deserialize::<RawRow>()
        .map(|row| {
            let row = row.map_err(|e| CatalogError::Row {
                line: e.position().map_or(0, |p| p.line()),
                error: e.to_string(),
            })?;
            Ok(CatalogEntry {
                id: row.id,
                kind: row.kind,
                issued: row.issued,
                title: row.title,
                languages: list(&row.language),
                authors: list(&row.authors),
                subjects: list(&row.subjects),
                locc: list(&row.locc),
                bookshelves: list(&row.bookshelves),
            })
        })
        .collect()
}

/// The catalogue separates list items with `; ` ("JK; KF"). Empty fields give empty lists.
fn list(field: &str) -> Vec<String> {
    field
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Which books to suggest. Every non-empty field must match; empty fields match everything.
/// Only `Text` entries ever match (no audio, images or datasets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Any of these language codes (`en`).
    pub languages: Vec<String>,
    /// Any Library of Congress class starting with one of these (`B` also matches `BL`, `BX`).
    pub locc: Vec<String>,
    /// Classes that don't count. A book is dropped only if none of its *other* classes match:
    /// philosophy that is also Catholic theology (`B; BX`) survives excluding `BX`.
    pub exclude_locc: Vec<String>,
    /// Appears in the authors field, ignoring case ("hume, david").
    pub author: Option<String>,
}

impl Query {
    pub fn matches(&self, entry: &CatalogEntry) -> bool {
        let any = |wanted: &[String], test: &dyn Fn(&str) -> bool| {
            wanted.is_empty() || wanted.iter().any(|w| test(w))
        };
        let counted = |class: &&String| !self.exclude_locc.iter().any(|x| class.starts_with(x));
        let classes: Vec<&String> = entry.locc.iter().filter(counted).collect();
        let excluded_everything = !entry.locc.is_empty() && classes.is_empty();
        entry.kind == "Text"
            && !excluded_everything
            && any(&self.languages, &|w| entry.languages.iter().any(|l| l == w))
            && any(&self.locc, &|w| classes.iter().any(|c| c.starts_with(w)))
            && self.author.as_deref().is_none_or(|a| {
                let a = a.to_lowercase();
                entry.authors.iter().any(|x| x.to_lowercase().contains(&a))
            })
    }
}
