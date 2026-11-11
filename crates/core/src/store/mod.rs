//! The storage interface. Everything outside this module talks to the `Store` trait, never to
//! Postgres directly, so the database can be swapped or faked without touching callers.

mod postgres;

use std::fmt;
use std::future::Future;

use crate::values::{InvalidValue, Slug, WikidataId};

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
}
