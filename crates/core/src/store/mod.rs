//! The storage interface. Everything outside this module talks to the `Store` trait, never to
//! Postgres directly, so the database can be swapped or faked without touching callers.

mod postgres;

use std::fmt;
use std::future::Future;

use time::OffsetDateTime;

use crate::values::{InvalidValue, Locator, Sha256, Slug, WikidataId};

pub use postgres::PgStore;

/// A thinker's internal database id. Typed so it can't be confused with another table's id.
/// Never shown publicly: URLs use the slug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThinkerId(pub(crate) i64);

/// What the importer knows about a thinker before it's stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThinker {
    pub slug: Slug,
    pub name: String,
    pub wikidata_id: Option<WikidataId>,
    /// Negative years are BCE (Epicurus: -341).
    pub born_year: Option<i16>,
    pub died_year: Option<i16>,
}

/// A stored thinker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thinker {
    pub id: ThinkerId,
    pub slug: Slug,
    pub name: String,
    pub wikidata_id: Option<WikidataId>,
    pub born_year: Option<i16>,
    pub died_year: Option<i16>,
}

impl Thinker {
    /// True if this stored row says exactly what `new` says.
    fn matches(&self, new: &NewThinker) -> bool {
        self.slug == new.slug
            && self.name == new.name
            && self.wikidata_id == new.wikidata_id
            && self.born_year == new.born_year
            && self.died_year == new.died_year
    }
}

/// A work's internal id (typed; never public).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkId(pub(crate) i64);

/// The abstract work, independent of any copy. Claims will attach here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWork {
    /// Primary author.
    pub thinker: ThinkerId,
    /// Unique per thinker: `/david-hume/<slug>`.
    pub slug: Slug,
    pub title: String,
    /// BCP 47 language tag of the original ("en", "la"); checked by the database.
    pub original_language: Option<String>,
    pub first_year: Option<i16>,
    pub wikidata_id: Option<WikidataId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub id: WorkId,
    pub thinker: ThinkerId,
    pub slug: Slug,
    pub title: String,
    pub original_language: Option<String>,
    pub first_year: Option<i16>,
    pub wikidata_id: Option<WikidataId>,
}

impl Work {
    fn matches(&self, new: &NewWork) -> bool {
        self.thinker == new.thinker
            && self.slug == new.slug
            && self.title == new.title
            && self.original_language == new.original_language
            && self.first_year == new.first_year
            && self.wikidata_id == new.wikidata_id
    }
}

/// A document's internal id (typed; never public).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub(crate) i64);

/// One stored file, exactly as downloaded. Its bytes live in object storage under `sha256`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDocument {
    pub sha256: Sha256,
    pub byte_length: i64,
    pub media_type: String,
    /// Where it came from: "gutenberg", "wikisource", …
    pub provider: Slug,
    /// The provider's own id: Gutenberg ebook "4583".
    pub provider_id: String,
    pub source_url: String,
    pub licence: String,
    /// When we downloaded it. Not compared on re-import: the first download is the record.
    pub retrieved_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub id: DocumentId,
    pub sha256: Sha256,
    pub byte_length: i64,
    pub media_type: String,
    pub provider: Slug,
    pub provider_id: String,
    pub source_url: String,
    pub licence: String,
    pub retrieved_at: OffsetDateTime,
}

impl Document {
    fn matches(&self, new: &NewDocument) -> bool {
        self.sha256 == new.sha256
            && self.byte_length == new.byte_length
            && self.media_type == new.media_type
            && self.provider == new.provider
            && self.provider_id == new.provider_id
            && self.source_url == new.source_url
            && self.licence == new.licence
    }
}

/// An edition's internal id (typed; never public).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EditionId(pub(crate) i64);

/// One work's text inside a document. Byte offsets are into the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEdition {
    pub work: WorkId,
    pub document: DocumentId,
    /// Unique per work: `?edition=<slug>`.
    pub slug: Slug,
    /// Title as printed in this edition.
    pub title: String,
    /// BCP 47 language of this text; checked by the database.
    pub language: String,
    pub translator: Option<String>,
    pub published_year: Option<i16>,
    pub start_byte: i64,
    pub end_byte: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edition {
    pub id: EditionId,
    pub work: WorkId,
    pub document: DocumentId,
    pub slug: Slug,
    pub title: String,
    pub language: String,
    pub translator: Option<String>,
    pub published_year: Option<i16>,
    pub start_byte: i64,
    pub end_byte: i64,
}

impl Edition {
    fn matches(&self, new: &NewEdition) -> bool {
        self.work == new.work
            && self.document == new.document
            && self.slug == new.slug
            && self.title == new.title
            && self.language == new.language
            && self.translator == new.translator
            && self.published_year == new.published_year
            && self.start_byte == new.start_byte
            && self.end_byte == new.end_byte
    }
}

/// A division's internal id (typed; never public).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DivisionId(pub(crate) i64);

/// One node of an edition's own structure: "Part 10", "Introduction", "Question 2".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDivision {
    pub edition: EditionId,
    /// `None` = top level.
    pub parent: Option<DivisionId>,
    /// `part`, `introduction`, `letter`, … (lowercase words joined by '-'; checked by the database).
    pub kind: String,
    /// As printed: "10", "Introduction".
    pub label: String,
    /// Position among its siblings.
    pub ordinal: i32,
    pub start_byte: i64,
    pub end_byte: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Division {
    pub id: DivisionId,
    pub edition: EditionId,
    pub parent: Option<DivisionId>,
    pub kind: String,
    pub label: String,
    pub ordinal: i32,
    pub start_byte: i64,
    pub end_byte: i64,
}

impl Division {
    fn matches(&self, new: &NewDivision) -> bool {
        self.edition == new.edition
            && self.parent == new.parent
            && self.kind == new.kind
            && self.label == new.label
            && self.ordinal == new.ordinal
            && self.start_byte == new.start_byte
            && self.end_byte == new.end_byte
    }
}

/// A passage's internal id (typed; never public).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassageId(pub(crate) i64);

/// Whose words a passage is. Editorial passages *must* say what they belong to — an
/// unattached editor note can't even be constructed. (The database enforces the same rule.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassageRole {
    /// The author's own words.
    Text,
    /// A heading an editor added to the section `of`.
    EditorHeading { of: PassageId },
    /// An editor's note on the passage `on`.
    EditorNote { on: PassageId },
}

impl PassageRole {
    /// The `passage.kind` value and the `attached_to` id.
    pub(crate) fn to_columns(self) -> (&'static str, Option<i64>) {
        match self {
            PassageRole::Text => ("text", None),
            PassageRole::EditorHeading { of } => ("editor-heading", Some(of.0)),
            PassageRole::EditorNote { on } => ("editor-note", Some(on.0)),
        }
    }

    pub(crate) fn from_columns(kind: &str, attached_to: Option<i64>) -> Result<Self, InvalidValue> {
        match (kind, attached_to) {
            ("text", None) => Ok(PassageRole::Text),
            ("editor-heading", Some(id)) => Ok(PassageRole::EditorHeading { of: PassageId(id) }),
            ("editor-note", Some(id)) => Ok(PassageRole::EditorNote { on: PassageId(id) }),
            _ => Err(InvalidValue { kind: "passage kind/attachment", value: format!("{kind} / {attached_to:?}") }),
        }
    }
}

/// The smallest citable unit. Its bytes in the document are the ground truth; `text` is a copy
/// of exactly those bytes (the store hashes it into `content_sha256`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPassage {
    pub edition: EditionId,
    pub division: Option<DivisionId>,
    /// Reading order within the edition.
    pub ordinal: i32,
    /// Work-level address shared across editions: `part-1/3`.
    pub locator: Locator,
    pub role: PassageRole,
    /// Offsets into the document.
    pub start_byte: i64,
    pub end_byte: i64,
    pub text: String,
    /// Hash of the normalised text, for finding the same passage elsewhere.
    pub normalized_sha256: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    pub id: PassageId,
    pub edition: EditionId,
    pub division: Option<DivisionId>,
    pub ordinal: i32,
    pub locator: Locator,
    pub role: PassageRole,
    pub start_byte: i64,
    pub end_byte: i64,
    pub text: String,
    /// SHA-256 of `text`'s bytes, computed by the store.
    pub content_sha256: Sha256,
    pub normalized_sha256: Sha256,
}

impl Passage {
    fn matches(&self, new: &NewPassage) -> bool {
        self.edition == new.edition
            && self.division == new.division
            && self.ordinal == new.ordinal
            && self.locator == new.locator
            && self.role == new.role
            && self.start_byte == new.start_byte
            && self.end_byte == new.end_byte
            && self.text == new.text
            && self.normalized_sha256 == new.normalized_sha256
    }
}

/// Everything that can go wrong in the store.
#[derive(Debug)]
pub enum StoreError {
    /// The row exists but says something different (or another row already holds a unique
    /// value, e.g. the same Wikidata id under another slug). Curated data is never overwritten.
    Conflict(String),
    /// Data read back from the database failed validation: the database is corrupt or a
    /// migration and this code disagree.
    Corrupt(InvalidValue),
    /// Connection, query or migration failure.
    Database(sqlx::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Conflict(what) => write!(f, "conflict: {what}"),
            StoreError::Corrupt(invalid) => write!(f, "corrupt data in database: {invalid}"),
            StoreError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

// `?` on a sqlx call turns its error into a StoreError: unique violations become Conflicts,
// everything else is a Database error.
impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        // A let-chain (Rust 2024): both conditions in one `if`.
        if let Some(db) = e.as_database_error()
            && db.is_unique_violation()
        {
            return StoreError::Conflict(db.message().to_owned());
        }
        StoreError::Database(e)
    }
}

impl From<InvalidValue> for StoreError {
    fn from(e: InvalidValue) -> Self {
        StoreError::Corrupt(e)
    }
}

/// The Library's storage operations.
///
/// Methods return `impl Future + Send` rather than being `async fn`: that promise ("this future
/// can move between threads") lets a multi-threaded server call the store from any task.
pub trait Store {
    /// Create the thinker, or return it unchanged if an identical row exists.
    /// A different row under the same slug (or the same Wikidata id) is a `Conflict`.
    fn ensure_thinker(&self, new: &NewThinker) -> impl Future<Output = Result<Thinker, StoreError>> + Send;

    fn thinker_by_slug(&self, slug: &Slug) -> impl Future<Output = Result<Option<Thinker>, StoreError>> + Send;

    /// Create the work, or return it unchanged. Conflicts like `ensure_thinker`.
    fn ensure_work(&self, new: &NewWork) -> impl Future<Output = Result<Work, StoreError>> + Send;

    fn work_by_slug(&self, thinker: ThinkerId, slug: &Slug)
    -> impl Future<Output = Result<Option<Work>, StoreError>> + Send;

    /// Record a downloaded file, or return the existing record. The same provider id with
    /// different bytes (the provider changed the file) is a `Conflict`, never a second copy.
    fn ensure_document(&self, new: &NewDocument) -> impl Future<Output = Result<Document, StoreError>> + Send;

    fn document_by_sha256(&self, sha256: &Sha256)
    -> impl Future<Output = Result<Option<Document>, StoreError>> + Send;

    /// Create the edition, or return it unchanged. Conflicts like `ensure_thinker`.
    fn ensure_edition(&self, new: &NewEdition) -> impl Future<Output = Result<Edition, StoreError>> + Send;

    /// Create the division, or return it unchanged. Conflicts like `ensure_thinker`.
    fn ensure_division(&self, new: &NewDivision) -> impl Future<Output = Result<Division, StoreError>> + Send;

    /// Create the passage, or return it unchanged. Conflicts like `ensure_thinker`.
    fn ensure_passage(&self, new: &NewPassage) -> impl Future<Output = Result<Passage, StoreError>> + Send;

    fn passage_by_locator(&self, edition: EditionId, locator: &Locator)
    -> impl Future<Output = Result<Option<Passage>, StoreError>> + Send;

    /// Delete an edition's divisions and passages (derived data) so it can be split again with
    /// changed rules. Deliberate and explicit: `ensure_*` never replaces anything on its own.
    /// Returns how many passages were removed.
    fn clear_edition_structure(&self, edition: EditionId) -> impl Future<Output = Result<u64, StoreError>> + Send;
}
