//! `Store` backed by PostgreSQL via sqlx. Queries use `sqlx::query!`, which checks the SQL and
//! its column types against the real schema at compile time (or the `.sqlx/` offline cache).

use sqlx::PgPool;
use time::OffsetDateTime;

use super::{
    Division, DivisionId, Document, DocumentId, Edition, EditionId, NewDivision, NewDocument, NewEdition, NewPassage,
    NewThinker, NewWork, Passage, PassageId, PassageRole, Store, StoreError, Thinker, ThinkerId, Work, WorkId,
};
use crate::values::{Locator, Sha256, Slug, WikidataId};

/// The error for "a row with this key exists but says something else".
fn conflict(what: &str, stored: &impl std::fmt::Debug, given: &impl std::fmt::Debug) -> StoreError {
    StoreError::Conflict(format!("{what} already exists with different data: stored {stored:?}, given {given:?}"))
}

/// Postgres-backed store. Cheap to clone: `PgPool` is a handle to a shared connection pool.
#[derive(Debug, Clone)]
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    /// Connect to `database_url` (e.g. the `DATABASE_URL` from `.env`).
    pub async fn connect(database_url: &str) -> Result<Self, StoreError> {
        Ok(PgStore { pool: PgPool::connect(database_url).await? })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        PgStore { pool }
    }

    /// Apply any pending migrations. They're embedded in the binary at compile time, so a
    /// deployed server can bring its own database up to date.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))
    }
}

/// A thinker row as sqlx returns it, before validation.
struct ThinkerRow {
    id: i64,
    slug: String,
    name: String,
    wikidata_id: Option<String>,
    born_year: Option<i16>,
    died_year: Option<i16>,
}

impl TryFrom<ThinkerRow> for Thinker {
    type Error = StoreError;

    fn try_from(row: ThinkerRow) -> Result<Self, Self::Error> {
        Ok(Thinker {
            id: ThinkerId(row.id),
            slug: Slug::parse(&row.slug)?,
            name: row.name,
            // `transpose` turns Option<Result<T, E>> into Result<Option<T>, E>, so `?` works.
            wikidata_id: row.wikidata_id.as_deref().map(WikidataId::parse).transpose()?,
            born_year: row.born_year,
            died_year: row.died_year,
        })
    }
}

struct WorkRow {
    id: i64,
    thinker_id: i64,
    slug: String,
    title: String,
    original_language: Option<String>,
    first_year: Option<i16>,
    wikidata_id: Option<String>,
}

impl TryFrom<WorkRow> for Work {
    type Error = StoreError;

    fn try_from(row: WorkRow) -> Result<Self, Self::Error> {
        Ok(Work {
            id: WorkId(row.id),
            thinker: ThinkerId(row.thinker_id),
            slug: Slug::parse(&row.slug)?,
            title: row.title,
            original_language: row.original_language,
            first_year: row.first_year,
            wikidata_id: row.wikidata_id.as_deref().map(WikidataId::parse).transpose()?,
        })
    }
}

struct DocumentRow {
    id: i64,
    sha256: Vec<u8>,
    byte_length: i64,
    media_type: String,
    provider: String,
    provider_id: String,
    source_url: String,
    licence: String,
    retrieved_at: OffsetDateTime,
}

impl TryFrom<DocumentRow> for Document {
    type Error = StoreError;

    fn try_from(row: DocumentRow) -> Result<Self, Self::Error> {
        Ok(Document {
            id: DocumentId(row.id),
            sha256: Sha256::from_slice(&row.sha256)?,
            byte_length: row.byte_length,
            media_type: row.media_type,
            provider: Slug::parse(&row.provider)?,
            provider_id: row.provider_id,
            source_url: row.source_url,
            licence: row.licence,
            retrieved_at: row.retrieved_at,
        })
    }
}

struct EditionRow {
    id: i64,
    work_id: i64,
    document_id: i64,
    slug: String,
    title: String,
    language: String,
    translator: Option<String>,
    published_year: Option<i16>,
    start_byte: i64,
    end_byte: i64,
}

impl TryFrom<EditionRow> for Edition {
    type Error = StoreError;

    fn try_from(row: EditionRow) -> Result<Self, Self::Error> {
        Ok(Edition {
            id: EditionId(row.id),
            work: WorkId(row.work_id),
            document: DocumentId(row.document_id),
            slug: Slug::parse(&row.slug)?,
            title: row.title,
            language: row.language,
            translator: row.translator,
            published_year: row.published_year,
            start_byte: row.start_byte,
            end_byte: row.end_byte,
        })
    }
}

struct DivisionRow {
    id: i64,
    edition_id: i64,
    parent_id: Option<i64>,
    kind: String,
    label: String,
    ordinal: i32,
    start_byte: i64,
    end_byte: i64,
}

impl From<DivisionRow> for Division {
    fn from(row: DivisionRow) -> Self {
        Division {
            id: DivisionId(row.id),
            edition: EditionId(row.edition_id),
            parent: row.parent_id.map(DivisionId),
            kind: row.kind,
            label: row.label,
            ordinal: row.ordinal,
            start_byte: row.start_byte,
            end_byte: row.end_byte,
        }
    }
}

struct PassageRow {
    id: i64,
    edition_id: i64,
    division_id: Option<i64>,
    ordinal: i32,
    locator: String,
    kind: String,
    attached_to: Option<i64>,
    start_byte: i64,
    end_byte: i64,
    text: String,
    content_sha256: Vec<u8>,
    normalized_sha256: Vec<u8>,
}

impl TryFrom<PassageRow> for Passage {
    type Error = StoreError;

    fn try_from(row: PassageRow) -> Result<Self, Self::Error> {
        Ok(Passage {
            id: PassageId(row.id),
            edition: EditionId(row.edition_id),
            division: row.division_id.map(DivisionId),
            ordinal: row.ordinal,
            locator: Locator::parse(&row.locator)?,
            role: PassageRole::from_columns(&row.kind, row.attached_to)?,
            start_byte: row.start_byte,
            end_byte: row.end_byte,
            text: row.text,
            content_sha256: Sha256::from_slice(&row.content_sha256)?,
            normalized_sha256: Sha256::from_slice(&row.normalized_sha256)?,
        })
    }
}

impl Store for PgStore {
    async fn ensure_thinker(&self, new: &NewThinker) -> Result<Thinker, StoreError> {
        // One round trip: try to insert. If a unique key (slug or Wikidata id) is taken, `ins`
        // is empty and the second half of the UNION returns the row that holds it; `matches`
        // then decides "same thinker again" vs Conflict. `"id!"` tells sqlx the column is
        // never NULL (it can't infer that through a UNION).
        let row = sqlx::query_as!(
            ThinkerRow,
            r#"
            WITH ins AS (
                INSERT INTO thinker (slug, name, wikidata_id, born_year, died_year)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT DO NOTHING
                RETURNING id, slug, name, wikidata_id, born_year, died_year
            )
            SELECT id AS "id!", slug AS "slug!", name AS "name!", wikidata_id, born_year, died_year
            FROM ins
            UNION ALL
            (SELECT id, slug, name, wikidata_id, born_year, died_year
             FROM thinker
             WHERE NOT EXISTS (SELECT 1 FROM ins) AND (slug = $1 OR wikidata_id = $3)
             LIMIT 1)
            "#,
            new.slug.as_str(),
            new.name,
            new.wikidata_id.as_ref().map(WikidataId::as_str),
            new.born_year,
            new.died_year,
        )
        .fetch_one(&self.pool)
        .await?;

        let thinker = Thinker::try_from(row)?;
        if thinker.matches(new) { Ok(thinker) } else { Err(conflict("thinker", &thinker, new)) }
    }

    async fn thinker_by_slug(&self, slug: &Slug) -> Result<Option<Thinker>, StoreError> {
        let row = sqlx::query_as!(
            ThinkerRow,
            "SELECT id, slug, name, wikidata_id, born_year, died_year FROM thinker WHERE slug = $1",
            slug.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;
        // Option<Row> → Option<Result<Thinker>> → Result<Option<Thinker>>.
        row.map(Thinker::try_from).transpose()
    }
    async fn ensure_work(&self, new: &NewWork) -> Result<Work, StoreError> {
        // Two unique keys can collide: (thinker_id, slug) and wikidata_id. `ON CONFLICT DO
        // NOTHING` without a target covers both; the fallback finds whichever row matched.
        let row = sqlx::query_as!(
            WorkRow,
            r#"
            WITH ins AS (
                INSERT INTO work (thinker_id, slug, title, original_language, first_year, wikidata_id)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT DO NOTHING
                RETURNING id, thinker_id, slug, title, original_language, first_year, wikidata_id
            )
            SELECT id AS "id!", thinker_id AS "thinker_id!", slug AS "slug!", title AS "title!",
                   original_language, first_year, wikidata_id
            FROM ins
            UNION ALL
            (SELECT id, thinker_id, slug, title, original_language, first_year, wikidata_id
             FROM work
             WHERE NOT EXISTS (SELECT 1 FROM ins)
               AND ((thinker_id = $1 AND slug = $2) OR wikidata_id = $6)
             LIMIT 1)
            "#,
            new.thinker.0,
            new.slug.as_str(),
            new.title,
            new.original_language,
            new.first_year,
            new.wikidata_id.as_ref().map(WikidataId::as_str),
        )
        .fetch_one(&self.pool)
        .await?;

        let work = Work::try_from(row)?;
        if work.matches(new) { Ok(work) } else { Err(conflict("work", &work, new)) }
    }

    async fn work_by_slug(&self, thinker: ThinkerId, slug: &Slug) -> Result<Option<Work>, StoreError> {
        let row = sqlx::query_as!(
            WorkRow,
            "SELECT id, thinker_id, slug, title, original_language, first_year, wikidata_id
             FROM work WHERE thinker_id = $1 AND slug = $2",
            thinker.0,
            slug.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(Work::try_from).transpose()
    }

    async fn ensure_document(&self, new: &NewDocument) -> Result<Document, StoreError> {
        // Unique keys: sha256, and (provider, provider_id). If the provider id is known but the
        // bytes differ, the fallback returns the old row, `matches` fails on the hash, and we
        // report a Conflict instead of storing a second copy.
        let row = sqlx::query_as!(
            DocumentRow,
            r#"
            WITH ins AS (
                INSERT INTO document (sha256, byte_length, media_type, provider, provider_id,
                                      source_url, licence, retrieved_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT DO NOTHING
                RETURNING id, sha256, byte_length, media_type, provider, provider_id,
                          source_url, licence, retrieved_at
            )
            SELECT id AS "id!", sha256 AS "sha256!", byte_length AS "byte_length!",
                   media_type AS "media_type!", provider AS "provider!", provider_id AS "provider_id!",
                   source_url AS "source_url!", licence AS "licence!", retrieved_at AS "retrieved_at!"
            FROM ins
            UNION ALL
            (SELECT id, sha256, byte_length, media_type, provider, provider_id,
                    source_url, licence, retrieved_at
             FROM document
             WHERE NOT EXISTS (SELECT 1 FROM ins)
               AND (sha256 = $1 OR (provider = $4 AND provider_id = $5))
             LIMIT 1)
            "#,
            new.sha256.as_bytes().as_slice(),
            new.byte_length,
            new.media_type,
            new.provider.as_str(),
            new.provider_id,
            new.source_url,
            new.licence,
            new.retrieved_at,
        )
        .fetch_one(&self.pool)
        .await?;

        let document = Document::try_from(row)?;
        if document.matches(new) { Ok(document) } else { Err(conflict("document", &document, new)) }
    }

    async fn document_by_sha256(&self, sha256: &Sha256) -> Result<Option<Document>, StoreError> {
        let row = sqlx::query_as!(
            DocumentRow,
            "SELECT id, sha256, byte_length, media_type, provider, provider_id, source_url, licence, retrieved_at
             FROM document WHERE sha256 = $1",
            sha256.as_bytes().as_slice(),
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(Document::try_from).transpose()
    }

    async fn ensure_edition(&self, new: &NewEdition) -> Result<Edition, StoreError> {
        // Unique keys: (document_id, work_id) and (work_id, slug).
        let row = sqlx::query_as!(
            EditionRow,
            r#"
            WITH ins AS (
                INSERT INTO edition (work_id, document_id, slug, title, language, translator,
                                     published_year, start_byte, end_byte)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT DO NOTHING
                RETURNING id, work_id, document_id, slug, title, language, translator,
                          published_year, start_byte, end_byte
            )
            SELECT id AS "id!", work_id AS "work_id!", document_id AS "document_id!", slug AS "slug!",
                   title AS "title!", language AS "language!", translator, published_year,
                   start_byte AS "start_byte!", end_byte AS "end_byte!"
            FROM ins
            UNION ALL
            (SELECT id, work_id, document_id, slug, title, language, translator,
                    published_year, start_byte, end_byte
             FROM edition
             WHERE NOT EXISTS (SELECT 1 FROM ins)
               AND ((document_id = $2 AND work_id = $1) OR (work_id = $1 AND slug = $3))
             LIMIT 1)
            "#,
            new.work.0,
            new.document.0,
            new.slug.as_str(),
            new.title,
            new.language,
            new.translator,
            new.published_year,
            new.start_byte,
            new.end_byte,
        )
        .fetch_one(&self.pool)
        .await?;

        let edition = Edition::try_from(row)?;
        if edition.matches(new) { Ok(edition) } else { Err(conflict("edition", &edition, new)) }
    }
    async fn edition_by_slug(&self, work: WorkId, slug: &Slug) -> Result<Option<Edition>, StoreError> {
        let row = sqlx::query_as!(
            EditionRow,
            "SELECT id, work_id, document_id, slug, title, language, translator, published_year, start_byte, end_byte
             FROM edition WHERE work_id = $1 AND slug = $2",
            work.0,
            slug.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(Edition::try_from).transpose()
    }

    async fn ensure_division(&self, new: &NewDivision) -> Result<Division, StoreError> {
        // Unique key: (edition_id, parent_id, ordinal) with NULLS NOT DISTINCT, so the fallback
        // compares parent_id with IS NOT DISTINCT FROM (plain `=` is never true for NULL).
        let row = sqlx::query_as!(
            DivisionRow,
            r#"
            WITH ins AS (
                INSERT INTO division (edition_id, parent_id, kind, label, ordinal, start_byte, end_byte)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT DO NOTHING
                RETURNING id, edition_id, parent_id, kind, label, ordinal, start_byte, end_byte
            )
            SELECT id AS "id!", edition_id AS "edition_id!", parent_id, kind AS "kind!", label AS "label!",
                   ordinal AS "ordinal!", start_byte AS "start_byte!", end_byte AS "end_byte!"
            FROM ins
            UNION ALL
            (SELECT id, edition_id, parent_id, kind, label, ordinal, start_byte, end_byte
             FROM division
             WHERE NOT EXISTS (SELECT 1 FROM ins)
               AND edition_id = $1 AND parent_id IS NOT DISTINCT FROM $2 AND ordinal = $5
             LIMIT 1)
            "#,
            new.edition.0,
            new.parent.map(|p| p.0),
            new.kind,
            new.label,
            new.ordinal,
            new.start_byte,
            new.end_byte,
        )
        .fetch_one(&self.pool)
        .await?;

        let division = Division::from(row);
        if division.matches(new) { Ok(division) } else { Err(conflict("division", &division, new)) }
    }

    async fn ensure_passage(&self, new: &NewPassage) -> Result<Passage, StoreError> {
        let (kind, attached_to) = new.role.to_columns();
        // Computed here, not passed in, so `text` and its hash can never disagree.
        let content_sha256 = Sha256::of(new.text.as_bytes());
        // Unique keys: (edition_id, start_byte, end_byte), (edition_id, ordinal), (edition_id, locator).
        let row = sqlx::query_as!(
            PassageRow,
            r#"
            WITH ins AS (
                INSERT INTO passage (edition_id, division_id, ordinal, locator, kind, attached_to,
                                     start_byte, end_byte, text, content_sha256, normalized_sha256)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT DO NOTHING
                RETURNING id, edition_id, division_id, ordinal, locator, kind, attached_to,
                          start_byte, end_byte, text, content_sha256, normalized_sha256
            )
            SELECT id AS "id!", edition_id AS "edition_id!", division_id, ordinal AS "ordinal!",
                   locator AS "locator!", kind AS "kind!", attached_to,
                   start_byte AS "start_byte!", end_byte AS "end_byte!", text AS "text!",
                   content_sha256 AS "content_sha256!", normalized_sha256 AS "normalized_sha256!"
            FROM ins
            UNION ALL
            (SELECT id, edition_id, division_id, ordinal, locator, kind, attached_to,
                    start_byte, end_byte, text, content_sha256, normalized_sha256
             FROM passage
             WHERE NOT EXISTS (SELECT 1 FROM ins)
               AND edition_id = $1
               AND (ordinal = $3 OR locator = $4 OR (start_byte = $7 AND end_byte = $8))
             LIMIT 1)
            "#,
            new.edition.0,
            new.division.map(|d| d.0),
            new.ordinal,
            new.locator.as_str(),
            kind,
            attached_to,
            new.start_byte,
            new.end_byte,
            new.text,
            content_sha256.as_bytes().as_slice(),
            new.normalized_sha256.as_bytes().as_slice(),
        )
        .fetch_one(&self.pool)
        .await?;

        let passage = Passage::try_from(row)?;
        if passage.matches(new) { Ok(passage) } else { Err(conflict("passage", &passage, new)) }
    }

    async fn passage_by_locator(&self, edition: EditionId, locator: &Locator) -> Result<Option<Passage>, StoreError> {
        let row = sqlx::query_as!(
            PassageRow,
            "SELECT id, edition_id, division_id, ordinal, locator, kind, attached_to,
                    start_byte, end_byte, text, content_sha256, normalized_sha256
             FROM passage WHERE edition_id = $1 AND locator = $2",
            edition.0,
            locator.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(Passage::try_from).transpose()
    }

    async fn clear_edition_structure(&self, edition: EditionId) -> Result<u64, StoreError> {
        // One transaction: either the whole structure goes, or none of it does.
        let mut tx = self.pool.begin().await?;
        let removed = sqlx::query!("DELETE FROM passage WHERE edition_id = $1", edition.0)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        sqlx::query!("DELETE FROM division WHERE edition_id = $1", edition.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(removed)
    }
}
