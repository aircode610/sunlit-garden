//! Store behaviour for divisions and passages, against real Postgres (fresh database per test).

use sqlx::PgPool;
use sunlit_core::store::{
    Edition, NewDivision, NewDocument, NewEdition, NewPassage, NewThinker, NewWork, PassageRole, PgStore, Store,
    StoreError,
};
use sunlit_core::{Locator, Sha256, Slug};
use time::OffsetDateTime;

/// Berkeley's Principles, Part I §3 — the candle passage.
const SECTION_3: &str = "That neither our thoughts, nor passions, nor ideas formed by the imagination, exist WITHOUT the mind...";
const HEADING_3: &str = "3. HOW FAR THE ASSENT OF THE VULGAR CONCEDED.--";
const NOTE_3: &str = "[Note: First argument in support of the author's theory.]";

async fn berkeley_edition(store: &PgStore) -> Edition {
    let slug = |s: &str| Slug::parse(s).unwrap();
    let thinker = store
        .ensure_thinker(&NewThinker { slug: slug("george-berkeley"), name: "George Berkeley".into(), wikidata_id: None, born_year: None, died_year: None })
        .await
        .unwrap();
    let work = store
        .ensure_work(&NewWork { thinker: thinker.id, slug: slug("principles-of-human-knowledge"), title: "Principles".into(), original_language: None, first_year: None, wikidata_id: None })
        .await
        .unwrap();
    let document = store
        .ensure_document(&NewDocument {
            sha256: Sha256::of(b"pg4723"),
            byte_length: 236260,
            media_type: "text/plain; charset=utf-8".into(),
            provider: slug("gutenberg"),
            provider_id: "4723".into(),
            source_url: "https://www.gutenberg.org/cache/epub/4723/pg4723.txt".into(),
            licence: "public-domain-us".into(),
            retrieved_at: OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    store
        .ensure_edition(&NewEdition {
            work: work.id,
            document: document.id,
            slug: slug("gutenberg-4723"),
            title: "Principles".into(),
            language: "en".into(),
            translator: None,
            published_year: None,
            start_byte: 1087,
            end_byte: 217292,
        })
        .await
        .unwrap()
}

fn part_1(edition: &Edition) -> NewDivision {
    NewDivision { edition: edition.id, parent: None, kind: "part".into(), label: "I".into(), ordinal: 1, start_byte: 40000, end_byte: 217292 }
}

fn passage(edition: &Edition, ordinal: i32, locator: &str, role: PassageRole, range: (i64, i64), text: &str) -> NewPassage {
    NewPassage {
        edition: edition.id,
        division: None,
        ordinal,
        locator: Locator::parse(locator).unwrap(),
        role,
        start_byte: range.0,
        end_byte: range.1,
        text: text.into(),
        normalized_sha256: Sha256::of(text.to_lowercase().as_bytes()),
    }
}

async fn count(pool: &PgPool, table: &'static str) -> i64 {
    let sql = sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}"));
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_section_with_its_editor_heading_and_note_round_trips(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let edition = berkeley_edition(&store).await;
    let part = store.ensure_division(&part_1(&edition)).await.unwrap();

    let section = store
        .ensure_passage(&NewPassage { division: Some(part.id), ..passage(&edition, 2, "part-1/3", PassageRole::Text, (47044, 48309), SECTION_3) })
        .await
        .unwrap();
    let heading = store
        .ensure_passage(&passage(&edition, 1, "part-1/3/heading", PassageRole::EditorHeading { of: section.id }, (46997, 47044), HEADING_3))
        .await
        .unwrap();
    let note = store
        .ensure_passage(&passage(&edition, 3, "part-1/3/note-1", PassageRole::EditorNote { on: section.id }, (48313, 48370), NOTE_3))
        .await
        .unwrap();

    let found = store.passage_by_locator(edition.id, &Locator::parse("part-1/3").unwrap()).await.unwrap();
    assert_eq!(found.as_ref(), Some(&section));
    assert_eq!(section.role, PassageRole::Text, "the author's words");
    assert_eq!(heading.role, PassageRole::EditorHeading { of: section.id });
    assert_eq!(note.role, PassageRole::EditorNote { on: section.id });
    assert_eq!(section.content_sha256, Sha256::of(SECTION_3.as_bytes()), "hash computed from the text");
}

#[sqlx::test(migrations = "../../migrations")]
async fn ensuring_the_same_structure_twice_is_a_no_op(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    let edition = berkeley_edition(&store).await;

    // Top-level division: parent is NULL, and in SQL `NULL = NULL` is not true.
    let first = store.ensure_division(&part_1(&edition)).await.unwrap();
    let again = store.ensure_division(&part_1(&edition)).await.unwrap();
    let p1 = store.ensure_passage(&passage(&edition, 0, "part-1/3", PassageRole::Text, (10, 20), SECTION_3)).await.unwrap();
    let p2 = store.ensure_passage(&passage(&edition, 0, "part-1/3", PassageRole::Text, (10, 20), SECTION_3)).await.unwrap();

    assert_eq!(first.id, again.id);
    assert_eq!(p1.id, p2.id);
    assert_eq!(count(&pool, "division").await, 1);
    assert_eq!(count(&pool, "passage").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_different_division_in_the_same_slot_is_a_conflict(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let edition = berkeley_edition(&store).await;
    store.ensure_division(&part_1(&edition)).await.unwrap();

    let result = store.ensure_division(&NewDivision { label: "Introduction".into(), ..part_1(&edition) }).await;

    assert!(matches!(result, Err(StoreError::Conflict(_))), "got {result:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn different_text_at_the_same_locator_is_a_conflict(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let edition = berkeley_edition(&store).await;
    store.ensure_passage(&passage(&edition, 0, "part-1/3", PassageRole::Text, (10, 20), SECTION_3)).await.unwrap();

    let result = store.ensure_passage(&passage(&edition, 0, "part-1/3", PassageRole::Text, (10, 20), "something else")).await;

    assert!(matches!(result, Err(StoreError::Conflict(_))), "got {result:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn clearing_an_editions_structure_removes_it_so_it_can_be_split_again(pool: PgPool) {
    let store = PgStore::from_pool(pool.clone());
    let edition = berkeley_edition(&store).await;
    let part = store.ensure_division(&part_1(&edition)).await.unwrap();
    let section = store
        .ensure_passage(&NewPassage { division: Some(part.id), ..passage(&edition, 0, "part-1/3", PassageRole::Text, (10, 20), SECTION_3) })
        .await
        .unwrap();
    store.ensure_passage(&passage(&edition, 1, "part-1/3/note-1", PassageRole::EditorNote { on: section.id }, (20, 30), NOTE_3)).await.unwrap();

    let removed = store.clear_edition_structure(edition.id).await.unwrap();

    assert_eq!(removed, 2);
    assert_eq!(count(&pool, "division").await, 0);
    assert_eq!(count(&pool, "passage").await, 0);
    assert_eq!(count(&pool, "edition").await, 1, "the edition itself is source of truth and stays");
    // And a new split with different text is accepted now.
    store.ensure_passage(&passage(&edition, 0, "part-1/3", PassageRole::Text, (10, 20), "re-split text")).await.unwrap();
}
