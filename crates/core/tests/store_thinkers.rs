//! Store behaviour for thinkers, against real Postgres. Each `#[sqlx::test]` gets a fresh
//! database with our migrations applied, and it's dropped afterwards if the test passes.
//! Needs DATABASE_URL (from .env) and the Compose Postgres running.

use sqlx::PgPool;
use sunlit_core::store::{NewThinker, PgStore, Store, StoreError};
use sunlit_core::{Slug, WikidataId};

fn hume() -> NewThinker {
    NewThinker {
        slug: Slug::parse("david-hume").unwrap(),
        name: "David Hume".into(),
        wikidata_id: Some(WikidataId::parse("Q37160").unwrap()),
        born_year: Some(1711),
        died_year: Some(1776),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn ensured_thinker_can_be_found_by_slug(pool: PgPool) {
    let store = PgStore::from_pool(pool);

    let stored = store.ensure_thinker(&hume()).await.unwrap();
    let found = store.thinker_by_slug(&hume().slug).await.unwrap();

    assert_eq!(found, Some(stored));
}

#[sqlx::test(migrations = "../../migrations")]
async fn ensuring_the_same_thinker_twice_is_a_no_op(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());

    let first = store.ensure_thinker(&hume()).await.unwrap();
    let second = store.ensure_thinker(&hume()).await.unwrap();

    assert_eq!(first, second, "a re-import must return the same row, same id");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM thinker").fetch_one(&pool).await.unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_slug_with_different_data_is_a_conflict(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    store.ensure_thinker(&hume()).await.unwrap();

    let typo = NewThinker { name: "David Hune".into(), ..hume() };
    let result = store.ensure_thinker(&typo).await;

    assert!(matches!(result, Err(StoreError::Conflict(_))), "got {result:?}");
    // And the curated row is untouched.
    let stored = store.thinker_by_slug(&hume().slug).await.unwrap().unwrap();
    assert_eq!(stored.name, "David Hume");
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_wikidata_id_under_another_slug_is_a_conflict(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    store.ensure_thinker(&hume()).await.unwrap();

    let duplicate = NewThinker { slug: Slug::parse("hume").unwrap(), ..hume() };
    let result = store.ensure_thinker(&duplicate).await;

    assert!(matches!(result, Err(StoreError::Conflict(_))), "got {result:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_slug_finds_nothing(pool: PgPool) {
    let store = PgStore::from_pool(pool);

    let found = store.thinker_by_slug(&Slug::parse("nobody").unwrap()).await.unwrap();

    assert_eq!(found, None);
}
