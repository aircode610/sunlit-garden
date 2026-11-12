//! Compile and store the real books end to end: import the real `corpus/sources.toml` (fixture
//! files served by a fake downloader), compile every edition, store the structure, and check
//! the rows in Postgres.

use bytes::Bytes;
use sqlx::PgPool;
use sunlit_core::blobs::BlobStore;
use sunlit_core::corpus::compile::compile_edition;
use sunlit_core::corpus::compile::store::{StoreStructureError, normalize_v1, store_structure};
use sunlit_core::corpus::compile::structure::Structure;
use sunlit_core::corpus::import::{FetchError, Fetcher, import};
use sunlit_core::corpus::manifest::{EditionEntry, Manifest};
use sunlit_core::store::{Edition, PassageRole, PgStore, Store};
use sunlit_core::{Locator, Sha256, Slug};

const SOURCES: &str = include_str!("../../../corpus/sources.toml");
const BERKELEY: &[u8] = include_bytes!("fixtures/gutenberg/pg4723.txt");
const HUME: &[u8] = include_bytes!("fixtures/gutenberg/pg4583.txt");

struct Fixtures;

impl Fetcher for Fixtures {
    async fn fetch(&self, url: &str) -> Result<Bytes, FetchError> {
        match url {
            u if u.ends_with("pg4723.txt") => Ok(Bytes::from_static(BERKELEY)),
            u if u.ends_with("pg4583.txt") => Ok(Bytes::from_static(HUME)),
            other => Err(FetchError(format!("no fixture for {other}"))),
        }
    }
}

/// Import everything, then return the stored edition, its file and its compiled structure.
async fn setup(store: &PgStore, edition_slug: &str) -> (Edition, &'static [u8], Structure, EditionEntry) {
    let manifest = Manifest::parse(SOURCES).unwrap();
    import(&manifest, store, &BlobStore::in_memory(), &Fixtures).await.unwrap();
    let (document, entry) = manifest
        .documents
        .iter()
        .find_map(|d| d.editions.iter().find(|e| e.slug.as_str() == edition_slug).map(|e| (d, e)))
        .unwrap();
    let file: &'static [u8] = if document.provider_id == "4723" { BERKELEY } else { HUME };
    let thinker = store.thinker_by_slug(&entry.work.0).await.unwrap().unwrap();
    let work = store.work_by_slug(thinker.id, &entry.work.1).await.unwrap().unwrap();
    let edition = store.edition_by_slug(work.id, &entry.slug).await.unwrap().unwrap();
    let structure = compile_edition(file, entry).unwrap();
    (edition, file, structure, entry.clone())
}

async fn scalar(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn berkeley_is_stored_completely(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    let (edition, file, structure, _) = setup(&store, "gutenberg-4723").await;

    let stored = store_structure(&store, &edition, file, &structure).await.unwrap();

    assert_eq!((stored.divisions, stored.passages), (4, 321));
    assert_eq!(scalar(&pool, "SELECT count(*) FROM division").await, 4);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage").await, 321);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage WHERE kind = 'text'").await, 186);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage WHERE kind = 'editor-heading'").await, 96);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage WHERE kind = 'editor-note'").await, 24);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage WHERE kind = 'editor-marker'").await, 15);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_candle_passage_and_its_editorial_material_are_linked(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let (edition, file, structure, _) = setup(&store, "gutenberg-4723").await;
    store_structure(&store, &edition, file, &structure).await.unwrap();
    let get = |loc: &'static str| {
        let store = store.clone();
        async move { store.passage_by_locator(edition.id, &Locator::parse(loc).unwrap()).await.unwrap().unwrap() }
    };

    let section = get("part-1/3").await;
    let heading = get("part-1/3/heading").await;
    let note = get("part-1/3/note-1").await;
    let marker = get("part-1/3/marker-1").await;

    assert!(section.text.starts_with("That neither our thoughts") && section.text.contains("Their ESSE is PERCIPI"));
    assert_eq!(section.text.as_bytes(), &file[section.start_byte as usize..section.end_byte as usize], "text = exact bytes");
    assert_eq!(section.role, PassageRole::Text);
    assert_eq!(heading.role, PassageRole::EditorHeading { of: section.id });
    assert_eq!(note.role, PassageRole::EditorNote { on: section.id });
    assert_eq!(marker.role, PassageRole::EditorMarker { in_: section.id });
    assert_eq!(marker.text, "[Note.]");
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_stored_insertion_lies_inside_its_host_and_every_text_is_exact(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    for slug in ["gutenberg-4723", "gutenberg-4583"] {
        let (edition, file, structure, _) = setup(&store, slug).await;
        store_structure(&store, &edition, file, &structure).await.unwrap();
    }

    // Markers and author notes are always printed inside their host; check it in the database.
    let outside = scalar(
        &pool,
        "SELECT count(*) FROM passage i JOIN passage h ON h.id = i.attached_to
         WHERE i.kind IN ('editor-marker', 'author-note')
           AND NOT (h.start_byte <= i.start_byte AND i.end_byte <= h.end_byte)",
    )
    .await;
    assert_eq!(outside, 0);
    // Editorial passages are attached to author text, never to each other.
    let bad_targets = scalar(
        &pool,
        "SELECT count(*) FROM passage i JOIN passage h ON h.id = i.attached_to WHERE h.kind <> 'text'",
    )
    .await;
    assert_eq!(bad_targets, 0);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage").await, 321 + 262);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage WHERE kind = 'author-note'").await, 10);
}

#[sqlx::test(migrations = "../../migrations")]
async fn storing_twice_changes_nothing(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    let (edition, file, structure, _) = setup(&store, "gutenberg-4583").await;

    store_structure(&store, &edition, file, &structure).await.unwrap();
    store_structure(&store, &edition, file, &structure).await.unwrap();

    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage").await, 262);
    assert_eq!(scalar(&pool, "SELECT count(*) FROM division").await, 13);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_edition_with_problems_is_refused_and_nothing_is_stored(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    let (edition, file, _, mut entry) = setup(&store, "gutenberg-4723").await;
    entry.compile.insertions.retain(|r| r.prefix != "[Vide"); // now an undeclared style
    let structure = compile_edition(file, &entry).unwrap();

    let result = store_structure(&store, &edition, file, &structure).await;

    assert!(matches!(result, Err(StoreStructureError::NeedsReview(n)) if n > 0), "got {result:?}");
    assert_eq!(scalar(&pool, "SELECT count(*) FROM passage").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn normalized_hash_is_of_the_authors_words_only(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let (edition, file, structure, _) = setup(&store, "gutenberg-4723").await;
    store_structure(&store, &edition, file, &structure).await.unwrap();

    let section = store.passage_by_locator(edition.id, &Locator::parse("part-1/22").unwrap()).await.unwrap().unwrap();
    let i = structure.passages.iter().position(|p| p.locator == "part-1/22").unwrap();
    let words = structure.author_words(file, i);

    assert!(section.text.contains("--Ed.]") && !words.contains("--Ed."));
    assert_eq!(section.normalized_sha256, Sha256::of(normalize_v1(&words).as_bytes()));
    assert_ne!(section.normalized_sha256, Sha256::of(normalize_v1(&section.text).as_bytes()));
}

#[test]
fn normalisation_v1_collapses_whitespace_and_lowercases() {
    assert_eq!(normalize_v1("  Their ESSE\r\n is   PERCIPI, "), "their esse is percipi,");
    let _ = Slug::parse("x"); // keep the import used across cfgs
}
