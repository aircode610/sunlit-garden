//! The importer end to end: source list → downloads → BlobStore + Postgres.
//! Uses a fake downloader (no network), in-memory blobs and a fresh Postgres per test.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use bytes::Bytes;
use sqlx::PgPool;
use sunlit_core::blobs::BlobStore;
use sunlit_core::corpus::import::{FetchError, Fetcher, ImportError, import};
use sunlit_core::corpus::manifest::Manifest;
use sunlit_core::Sha256;
use sunlit_core::store::PgStore;

const HUME_URL: &str = "https://www.gutenberg.org/cache/epub/4583/pg4583.txt";
const HUME_FILE: &str = "The Project Gutenberg eBook of Dialogues\r\n\r\n\
    *** START OF THE PROJECT GUTENBERG EBOOK DIALOGUES CONCERNING NATURAL RELIGION ***\r\n\r\n\
    Produced by Col Choat.\r\n\r\n\
    Dialogues Concerning Natural Religion\r\n\r\nPAMPHILUS TO HERMIPPUS\r\n\r\n\
    *** END OF THE PROJECT GUTENBERG EBOOK DIALOGUES CONCERNING NATURAL RELIGION ***\r\nfooter\r\n";

/// Serves fixed bytes per URL and counts downloads.
struct FakeFetcher {
    files: HashMap<String, Bytes>,
    downloads: AtomicUsize,
}

impl FakeFetcher {
    fn new(files: &[(&str, &str)]) -> Self {
        let files = files.iter().map(|(url, body)| (url.to_string(), Bytes::from(body.to_string()))).collect();
        FakeFetcher { files, downloads: AtomicUsize::new(0) }
    }

    fn downloads(&self) -> usize {
        self.downloads.load(Ordering::SeqCst)
    }
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, url: &str) -> Result<Bytes, FetchError> {
        self.downloads.fetch_add(1, Ordering::SeqCst);
        self.files.get(url).cloned().ok_or_else(|| FetchError(format!("404 {url}")))
    }
}

/// Hume's entry, pinned to `pin` (or unpinned if None).
fn hume_manifest(pin: Option<Sha256>) -> Manifest {
    let sha_line = pin.map(|s| format!("sha256 = \"{}\"\n", s.to_hex())).unwrap_or_default();
    Manifest::parse(&format!(
        r#"
[[thinker]]
slug = "david-hume"
name = "David Hume"
wikidata = "Q37160"

[[work]]
thinker = "david-hume"
slug = "dialogues-concerning-natural-religion"
title = "Dialogues Concerning Natural Religion"
wikidata = "Q965564"

[[document]]
provider = "gutenberg"
provider_id = "4583"
url = "{HUME_URL}"
licence = "public-domain-us"
media_type = "text/plain; charset=utf-8"
{sha_line}
[[document.edition]]
work = "david-hume/dialogues-concerning-natural-religion"
slug = "gutenberg-4583"
title = "Dialogues Concerning Natural Religion"
language = "en"
"#
    ))
    .unwrap()
}

async fn count(pool: &PgPool, table: &'static str) -> i64 {
    let sql = sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}"));
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn importing_hume_stores_the_file_and_all_rows(pool: PgPool) {
    let (store, blobs) = (PgStore::from_pool(pool.clone()), BlobStore::in_memory());
    let fetcher = FakeFetcher::new(&[(HUME_URL, HUME_FILE)]);
    let pin = Sha256::of(HUME_FILE.as_bytes());

    let report = import(&hume_manifest(Some(pin)), &store, &blobs, &fetcher).await.unwrap();

    assert_eq!(blobs.get(&pin).await.unwrap(), HUME_FILE.as_bytes(), "raw bytes stored untouched");
    for table in ["thinker", "work", "document", "edition"] {
        assert_eq!(count(&pool, table).await, 1, "{table}");
    }
    // The edition is the book, not Gutenberg's header, credit or footer.
    let range = report.documents[0].editions[0].1.clone();
    assert!(HUME_FILE[range].starts_with("Dialogues Concerning Natural Religion"));
    let (start, end): (i64, i64) = sqlx::query_as("SELECT start_byte, end_byte FROM edition").fetch_one(&pool).await.unwrap();
    assert!(HUME_FILE[start as usize..end as usize].ends_with("PAMPHILUS TO HERMIPPUS"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn re_running_the_import_downloads_nothing_and_changes_nothing(pool: PgPool) {
    let (store, blobs) = (PgStore::from_pool(pool.clone()), BlobStore::in_memory());
    let fetcher = FakeFetcher::new(&[(HUME_URL, HUME_FILE)]);
    let manifest = hume_manifest(Some(Sha256::of(HUME_FILE.as_bytes())));

    import(&manifest, &store, &blobs, &fetcher).await.unwrap();
    let report = import(&manifest, &store, &blobs, &fetcher).await.unwrap();

    assert_eq!(fetcher.downloads(), 1, "second run must use the stored blob");
    assert!(!report.documents[0].downloaded);
    for table in ["thinker", "work", "document", "edition"] {
        assert_eq!(count(&pool, table).await, 1, "{table}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_download_that_does_not_match_its_pin_is_refused_and_not_stored(pool: PgPool) {
    let (store, blobs) = (PgStore::from_pool(pool.clone()), BlobStore::in_memory());
    let changed = HUME_FILE.replace("PAMPHILUS", "PAMPHILVS"); // upstream edited the file
    let fetcher = FakeFetcher::new(&[(HUME_URL, &changed)]);
    let pin = Sha256::of(HUME_FILE.as_bytes());

    let result = import(&hume_manifest(Some(pin)), &store, &blobs, &fetcher).await;

    assert!(matches!(&result, Err(ImportError::HashMismatch { pinned, .. }) if *pinned == pin), "got {result:?}");
    assert!(!blobs.contains(&Sha256::of(changed.as_bytes())).await.unwrap());
    assert_eq!(count(&pool, "document").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unpinned_documents_write_nothing_and_report_the_observed_hash(pool: PgPool) {
    let (store, blobs) = (PgStore::from_pool(pool.clone()), BlobStore::in_memory());
    let fetcher = FakeFetcher::new(&[(HUME_URL, HUME_FILE)]);

    let result = import(&hume_manifest(None), &store, &blobs, &fetcher).await;

    let Err(ImportError::Unpinned(observed)) = result else { panic!("got {result:?}") };
    assert_eq!(observed, vec![(HUME_URL.to_string(), Sha256::of(HUME_FILE.as_bytes()))]);
    assert_eq!(count(&pool, "thinker").await, 0, "nothing written while anything is unpinned");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_explicit_range_past_the_end_of_the_file_is_refused(pool: PgPool) {
    let (store, blobs) = (PgStore::from_pool(pool.clone()), BlobStore::in_memory());
    let fetcher = FakeFetcher::new(&[(HUME_URL, HUME_FILE)]);
    let pin = Sha256::of(HUME_FILE.as_bytes());
    let mut manifest = hume_manifest(Some(pin));
    manifest.documents[0].editions[0].range =
        sunlit_core::corpus::manifest::RangeSpec::Bytes { start: 0, end: HUME_FILE.len() as i64 + 1 };

    let result = import(&manifest, &store, &blobs, &fetcher).await;

    assert!(matches!(result, Err(ImportError::Range { .. })), "got {result:?}");
    assert_eq!(count(&pool, "edition").await, 0);
}
