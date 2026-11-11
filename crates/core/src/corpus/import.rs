//! The importer: a validated source list → raw files in the BlobStore → rows in the Store.
//!
//! Safe to run any number of times: every write is an idempotent `ensure_*` or a
//! content-addressed `put`, and a pinned file already in the BlobStore isn't downloaded again.
//! If a run fails part-way, fix the cause and re-run; completed work is simply re-confirmed.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::ops::Range;

use bytes::Bytes;
use time::OffsetDateTime;

use super::gutenberg::{self, GutenbergError};
use super::manifest::{DocumentEntry, Manifest, RangeSpec};
use crate::blobs::{BlobError, BlobStore};
use crate::store::{NewDocument, NewEdition, NewThinker, NewWork, Store, StoreError, ThinkerId, WorkId};
use crate::values::{Sha256, Slug};

/// Downloads a URL. The CLI implements it over HTTPS; tests use a fake.
pub trait Fetcher {
    fn fetch(&self, url: &str) -> impl Future<Output = Result<Bytes, FetchError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError(pub String);

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What one import run did.
#[derive(Debug)]
pub struct Report {
    pub documents: Vec<DocumentReport>,
}

#[derive(Debug)]
pub struct DocumentReport {
    pub provider_id: String,
    pub sha256: Sha256,
    /// False when the pinned file was already stored and nothing was downloaded.
    pub downloaded: bool,
    /// Each edition and its byte range in the file.
    pub editions: Vec<(Slug, Range<usize>)>,
}

#[derive(Debug)]
pub enum ImportError {
    Fetch { url: String, error: FetchError },
    /// Documents without a pinned hash. Nothing was written; review and pin these.
    Unpinned(Vec<(String, Sha256)>),
    /// The download isn't the file we pinned: the provider changed it, or something tampered.
    HashMismatch { url: String, pinned: Sha256, observed: Sha256 },
    Gutenberg { url: String, error: GutenbergError },
    /// An explicit edition range doesn't fit inside the file.
    Range { url: String, edition: Slug, start: i64, end: i64, file_length: usize },
    Store(StoreError),
    Blob(BlobError),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Fetch { url, error } => write!(f, "download failed for {url}: {error}"),
            ImportError::Unpinned(observed) => {
                writeln!(f, "{} document(s) have no pinned sha256; nothing was imported.", observed.len())?;
                writeln!(f, "Review each file, then add these lines to corpus/sources.toml:")?;
                for (url, sha) in observed {
                    writeln!(f, "  {url}\n    sha256 = \"{}\"", sha.to_hex())?;
                }
                Ok(())
            }
            ImportError::HashMismatch { url, pinned, observed } => write!(
                f,
                "{url} is not the pinned file: pinned {}, downloaded {}. \
                 The provider may have changed it; review before re-pinning.",
                pinned.to_hex(),
                observed.to_hex()
            ),
            ImportError::Gutenberg { url, error } => write!(f, "{url}: {error}"),
            ImportError::Range { url, edition, start, end, file_length } => write!(
                f,
                "{url}, edition {}: range {start}..{end} doesn't fit a {file_length}-byte file",
                edition.as_str()
            ),
            ImportError::Store(e) => write!(f, "{e}"),
            ImportError::Blob(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<StoreError> for ImportError {
    fn from(e: StoreError) -> Self {
        ImportError::Store(e)
    }
}

impl From<BlobError> for ImportError {
    fn from(e: BlobError) -> Self {
        ImportError::Blob(e)
    }
}

/// Import everything in `manifest`.
pub async fn import<S: Store, F: Fetcher>(
    manifest: &Manifest,
    store: &S,
    blobs: &BlobStore,
    fetcher: &F,
) -> Result<Report, ImportError> {
    // 1. Refuse before writing anything if any document isn't pinned, reporting all of them.
    let mut unpinned = Vec::new();
    for document in manifest.documents.iter().filter(|d| d.sha256.is_none()) {
        let bytes = fetch(fetcher, &document.url).await?;
        unpinned.push((document.url.clone(), Sha256::of(&bytes)));
    }
    if !unpinned.is_empty() {
        return Err(ImportError::Unpinned(unpinned));
    }

    // 2. Thinkers, then works (which need their thinker's id).
    let mut thinker_ids: HashMap<&Slug, ThinkerId> = HashMap::new();
    for t in &manifest.thinkers {
        let new = NewThinker {
            slug: t.slug.clone(),
            name: t.name.clone(),
            wikidata_id: t.wikidata_id.clone(),
            born_year: t.born_year,
            died_year: t.died_year,
        };
        thinker_ids.insert(&t.slug, store.ensure_thinker(&new).await?.id);
    }
    let mut work_ids: HashMap<(&Slug, &Slug), WorkId> = HashMap::new();
    for w in &manifest.works {
        let new = NewWork {
            // Validation guarantees every work's thinker is listed.
            thinker: thinker_ids[&w.thinker],
            slug: w.slug.clone(),
            title: w.title.clone(),
            original_language: w.original_language.clone(),
            first_year: w.first_year,
            wikidata_id: w.wikidata_id.clone(),
        };
        work_ids.insert((&w.thinker, &w.slug), store.ensure_work(&new).await?.id);
    }

    // 3. Documents and the editions inside them.
    let mut documents = Vec::new();
    for document in &manifest.documents {
        documents.push(import_document(document, &work_ids, store, blobs, fetcher).await?);
    }
    Ok(Report { documents })
}

async fn import_document<S: Store, F: Fetcher>(
    document: &DocumentEntry,
    work_ids: &HashMap<(&Slug, &Slug), WorkId>,
    store: &S,
    blobs: &BlobStore,
    fetcher: &F,
) -> Result<DocumentReport, ImportError> {
    let pinned = document.sha256.expect("unpinned documents were refused in step 1");

    // Reuse the stored file if we have it (`get` re-verifies the hash); otherwise download,
    // check against the pin, and only then store it.
    let (bytes, downloaded) = if blobs.contains(&pinned).await? {
        (blobs.get(&pinned).await?, false)
    } else {
        let bytes = fetch(fetcher, &document.url).await?;
        let observed = Sha256::of(&bytes);
        if observed != pinned {
            return Err(ImportError::HashMismatch { url: document.url.clone(), pinned, observed });
        }
        blobs.put(bytes.clone()).await?;
        (bytes, true)
    };

    let stored = store
        .ensure_document(&NewDocument {
            sha256: pinned,
            byte_length: bytes.len() as i64,
            media_type: document.media_type.clone(),
            provider: document.provider.clone(),
            provider_id: document.provider_id.clone(),
            source_url: document.url.clone(),
            licence: document.licence.clone(),
            retrieved_at: OffsetDateTime::now_utc(),
        })
        .await?;

    let mut editions = Vec::new();
    for edition in &document.editions {
        let range = edition_range(edition.range, &bytes, &document.url, &edition.slug)?;
        let (thinker, work) = (&edition.work.0, &edition.work.1);
        store
            .ensure_edition(&NewEdition {
                work: work_ids[&(thinker, work)],
                document: stored.id,
                slug: edition.slug.clone(),
                title: edition.title.clone(),
                language: edition.language.clone(),
                translator: edition.translator.clone(),
                published_year: edition.published_year,
                start_byte: range.start as i64,
                end_byte: range.end as i64,
            })
            .await?;
        editions.push((edition.slug.clone(), range));
    }

    Ok(DocumentReport { provider_id: document.provider_id.clone(), sha256: pinned, downloaded, editions })
}

async fn fetch<F: Fetcher>(fetcher: &F, url: &str) -> Result<Bytes, ImportError> {
    fetcher.fetch(url).await.map_err(|error| ImportError::Fetch { url: url.to_owned(), error })
}

fn edition_range(spec: RangeSpec, bytes: &[u8], url: &str, edition: &Slug) -> Result<Range<usize>, ImportError> {
    match spec {
        RangeSpec::GutenbergBody => {
            gutenberg::body_range(bytes).map_err(|error| ImportError::Gutenberg { url: url.to_owned(), error })
        }
        RangeSpec::Bytes { start, end } if end as usize <= bytes.len() => Ok(start as usize..end as usize),
        RangeSpec::Bytes { start, end } => Err(ImportError::Range {
            url: url.to_owned(),
            edition: edition.clone(),
            start,
            end,
            file_length: bytes.len(),
        }),
    }
}
