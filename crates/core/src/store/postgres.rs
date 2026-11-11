//! `Store` backed by PostgreSQL via sqlx. Queries use `sqlx::query!`, which checks the SQL and
//! its column types against the real schema at compile time (or the `.sqlx/` offline cache).

use sqlx::PgPool;

use super::{NewThinker, Store, StoreError, Thinker, ThinkerId};
use crate::values::{Slug, WikidataId};

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

impl Store for PgStore {
    async fn ensure_thinker(&self, new: &NewThinker) -> Result<Thinker, StoreError> {
        // One round trip: try to insert; if the slug is taken, `ins` is empty and the second
        // half of the UNION returns the existing row instead. `"id!"` tells sqlx the column
        // is never NULL (it can't infer that through a UNION).
        let row = sqlx::query_as!(
            ThinkerRow,
            r#"
            WITH ins AS (
                INSERT INTO thinker (slug, name, wikidata_id, born_year, died_year)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (slug) DO NOTHING
                RETURNING id, slug, name, wikidata_id, born_year, died_year
            )
            SELECT id AS "id!", slug AS "slug!", name AS "name!", wikidata_id, born_year, died_year
            FROM ins
            UNION ALL
            SELECT id, slug, name, wikidata_id, born_year, died_year
            FROM thinker
            WHERE slug = $1 AND NOT EXISTS (SELECT 1 FROM ins)
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
        if thinker.matches(new) {
            Ok(thinker)
        } else {
            Err(StoreError::Conflict(format!(
                "thinker {:?} already exists with different data: stored {thinker:?}, given {new:?}",
                new.slug.as_str()
            )))
        }
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
}
