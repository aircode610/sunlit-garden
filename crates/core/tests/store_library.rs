//! Store behaviour for works, documents and editions — the real cases the importer will hit.
//! Runs against real Postgres; each test gets a fresh migrated database.

use sqlx::PgPool;
use sunlit_core::store::{
    Document, NewDocument, NewEdition, NewThinker, NewWork, PgStore, Store, StoreError, Thinker, Work,
};
use sunlit_core::{Sha256, Slug, WikidataId};
use time::OffsetDateTime;
use time::macros::datetime;

fn slug(s: &str) -> Slug {
    Slug::parse(s).unwrap()
}

async fn thinker(store: &PgStore, slug_: &str, name: &str) -> Thinker {
    let new = NewThinker { slug: slug(slug_), name: name.into(), wikidata_id: None, born_year: None, died_year: None };
    store.ensure_thinker(&new).await.unwrap()
}

fn new_work(author: &Thinker, slug_: &str, title: &str) -> NewWork {
    NewWork {
        thinker: author.id,
        slug: slug(slug_),
        title: title.into(),
        original_language: Some("en".into()),
        first_year: None,
        wikidata_id: None,
    }
}

/// Gutenberg #4583 as downloaded at a given time, with given bytes.
fn gutenberg_4583(bytes: &[u8], retrieved_at: OffsetDateTime) -> NewDocument {
    NewDocument {
        sha256: Sha256::of(bytes),
        byte_length: bytes.len() as i64,
        media_type: "text/plain; charset=utf-8".into(),
        provider: slug("gutenberg"),
        provider_id: "4583".into(),
        source_url: "https://www.gutenberg.org/ebooks/4583".into(),
        licence: "public-domain-us".into(),
        retrieved_at,
    }
}

/// Row count of one of our tables. `table` only ever comes from string literals in these
/// tests, never from input, so building SQL from it is safe — `AssertSqlSafe` records that
/// judgement (sqlx 0.9 refuses `format!`-built SQL without it).
async fn count(pool: &PgPool, table: &'static str) -> i64 {
    let sql = sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}"));
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn importing_hume_twice_changes_nothing(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    let text = b"DIALOGUES CONCERNING NATURAL RELIGION ...";

    // Run the whole import chain twice, as a re-run of the importer would.
    let mut ids = Vec::new();
    for _ in 0..2 {
        let hume = thinker(&store, "david-hume", "David Hume").await;
        let work = store
            .ensure_work(&NewWork {
                wikidata_id: Some(WikidataId::parse("Q1213006").unwrap()),
                first_year: Some(1779),
                ..new_work(&hume, "dialogues-concerning-natural-religion", "Dialogues Concerning Natural Religion")
            })
            .await
            .unwrap();
        let document = store.ensure_document(&gutenberg_4583(text, OffsetDateTime::now_utc())).await.unwrap();
        let edition = store
            .ensure_edition(&NewEdition {
                work: work.id,
                document: document.id,
                slug: slug("gutenberg-4583"),
                title: "Dialogues Concerning Natural Religion".into(),
                language: "en".into(),
                translator: None,
                published_year: None,
                start_byte: 0,
                end_byte: text.len() as i64,
            })
            .await
            .unwrap();
        ids.push((hume.id, work.id, document.id, edition.id));
    }

    assert_eq!(ids[0], ids[1], "second run must return the same rows");
    for table in ["thinker", "work", "document", "edition"] {
        assert_eq!(count(&pool, table).await, 1, "{table}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn re_downloading_the_same_file_keeps_the_first_download_time(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let first = datetime!(2026-11-11 09:00 UTC);

    let original = store.ensure_document(&gutenberg_4583(b"same bytes", first)).await.unwrap();
    let again = store.ensure_document(&gutenberg_4583(b"same bytes", datetime!(2026-12-01 09:00 UTC))).await.unwrap();

    assert_eq!(again.id, original.id);
    assert_eq!(again.retrieved_at, first);
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_changing_the_file_is_a_conflict_not_a_second_copy(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    let original: Document = store.ensure_document(&gutenberg_4583(b"version 1", OffsetDateTime::now_utc())).await.unwrap();

    // Gutenberg fixed a typo: same ebook number, different bytes.
    let result = store.ensure_document(&gutenberg_4583(b"version 2", OffsetDateTime::now_utc())).await;

    assert!(matches!(result, Err(StoreError::Conflict(_))), "got {result:?}");
    assert_eq!(count(&pool, "document").await, 1);
    let stored = store.document_by_sha256(&Sha256::of(b"version 1")).await.unwrap();
    assert_eq!(stored, Some(original), "the original record is untouched");
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_document_can_hold_several_works(pool: PgPool) {
    // Gutenberg's Anselm volume: the Proslogion and Gaunilo's reply in one file.
    let store = PgStore::from_pool(pool.clone());
    let anselm = thinker(&store, "anselm-of-canterbury", "Anselm of Canterbury").await;
    let gaunilo = thinker(&store, "gaunilo-of-marmoutiers", "Gaunilo of Marmoutiers").await;
    let proslogion = store.ensure_work(&new_work(&anselm, "proslogion", "Proslogion")).await.unwrap();
    let reply = store.ensure_work(&new_work(&gaunilo, "on-behalf-of-the-fool", "On Behalf of the Fool")).await.unwrap();
    let volume = store
        .ensure_document(&NewDocument { provider_id: "anselm-volume".into(), ..gutenberg_4583(b"PROSLOGIUM ... IN BEHALF OF THE FOOL ...", OffsetDateTime::now_utc()) })
        .await
        .unwrap();

    for (work, range) in [(&proslogion, (0, 14)), (&reply, (14, 40))] {
        let edition = NewEdition {
            work: work.id,
            document: volume.id,
            slug: slug("gutenberg-anselm-volume"),
            title: work.title.clone(),
            language: "en".into(),
            translator: Some("Sidney Norton Deane".into()),
            published_year: Some(1903),
            start_byte: range.0,
            end_byte: range.1,
        };
        store.ensure_edition(&edition).await.unwrap();
    }

    assert_eq!(count(&pool, "document").await, 1);
    assert_eq!(count(&pool, "edition").await, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn work_slugs_are_unique_per_thinker_only(pool: PgPool) {
    // Hume's and Berkeley's "dialogues" can both exist: their URLs differ by thinker.
    let store = PgStore::from_pool(pool);
    let hume = thinker(&store, "david-hume", "David Hume").await;
    let berkeley = thinker(&store, "george-berkeley", "George Berkeley").await;

    let a: Work = store.ensure_work(&new_work(&hume, "dialogues", "Dialogues Concerning Natural Religion")).await.unwrap();
    let b: Work = store.ensure_work(&new_work(&berkeley, "dialogues", "Three Dialogues between Hylas and Philonous")).await.unwrap();

    assert_ne!(a.id, b.id);
    assert_eq!(store.work_by_slug(berkeley.id, &slug("dialogues")).await.unwrap(), Some(b));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_edition_can_be_found_by_its_work_and_slug(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let hume = thinker(&store, "david-hume", "David Hume").await;
    let work = store.ensure_work(&new_work(&hume, "dialogues", "Dialogues")).await.unwrap();
    let document = store.ensure_document(&gutenberg_4583(b"text", OffsetDateTime::now_utc())).await.unwrap();
    let edition = store
        .ensure_edition(&NewEdition {
            work: work.id,
            document: document.id,
            slug: slug("gutenberg-4583"),
            title: "Dialogues".into(),
            language: "en".into(),
            translator: None,
            published_year: None,
            start_byte: 0,
            end_byte: 4,
        })
        .await
        .unwrap();

    assert_eq!(store.edition_by_slug(work.id, &slug("gutenberg-4583")).await.unwrap(), Some(edition));
    assert_eq!(store.edition_by_slug(work.id, &slug("other")).await.unwrap(), None);
}
