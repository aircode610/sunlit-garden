-- Migration 1: the Library (M0).
--
--   thinker ──< work ──< edition >── document          (a document is one stored file)
--                          └──< division (tree) ──< passage ──< passage_label
--
-- Rules (see the vault's Decisions.md):
--   * Internal ids are bigint identities and never leave the database; public URLs use slugs.
--   * Raw files live in object storage keyed by SHA-256; `document` records them.
--   * Byte ranges into the document are the ground truth for every citation.
--   * Uniqueness constraints make accidental duplicates impossible; imports use ON CONFLICT.
--   * Source-of-truth rows (thinker, work, document) are protected from deletion (RESTRICT);
--     derived rows (division, passage, label) are rebuilt from them and cascade.
--
-- Shared CHECK patterns:
--   slug     ^[a-z0-9]+(-[a-z0-9]+)*$              e.g. david-hume, gutenberg-4583
--   locator  slugs joined by '/'                    e.g. part-10/3, introduction/3
--   qid      ^Q[1-9][0-9]*$                         Wikidata item id, e.g. Q37160
--   lang     BCP 47-ish ^[a-z]{2,3}(-[A-Za-z0-9]+)*$ e.g. en, la, fr, en-GB

-- ---------------------------------------------------------------------------
-- thinker: a person.
-- ---------------------------------------------------------------------------
CREATE TABLE thinker (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    slug        text NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    name        text NOT NULL CHECK (name <> ''),        -- display name, "David Hume"
    wikidata_id text UNIQUE CHECK (wikidata_id ~ '^Q[1-9][0-9]*$'),
    born_year   smallint,                                -- negative = BCE (Epicurus: -341)
    died_year   smallint,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CHECK (born_year IS NULL OR died_year IS NULL OR born_year <= died_year)
);

-- Spelling variants ("Hume", "Hume, David"). `alias_key` is the alias normalised in Rust
-- (Unicode NFKC + case-fold + collapsed whitespace) so lookups ignore case and accents.
-- Not unique on its own: two thinkers can share a surname.
CREATE TABLE thinker_alias (
    thinker_id bigint NOT NULL REFERENCES thinker (id) ON DELETE CASCADE,
    alias      text NOT NULL CHECK (alias <> ''),
    alias_key  text NOT NULL CHECK (alias_key <> ''),
    PRIMARY KEY (thinker_id, alias_key)
);
CREATE INDEX thinker_alias_key_idx ON thinker_alias (alias_key);

-- ---------------------------------------------------------------------------
-- work: the abstract work, independent of any copy. Claims attach here.
-- One primary author; co-authorship is rare in this corpus (revisit if needed).
-- ---------------------------------------------------------------------------
CREATE TABLE work (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    thinker_id        bigint NOT NULL REFERENCES thinker (id) ON DELETE RESTRICT,
    slug              text NOT NULL CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    title             text NOT NULL CHECK (title <> ''),
    original_language text CHECK (original_language ~ '^[a-z]{2,3}(-[A-Za-z0-9]+)*$'),
    first_year        smallint,                          -- first publication / composition
    wikidata_id       text UNIQUE CHECK (wikidata_id ~ '^Q[1-9][0-9]*$'),
    created_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (thinker_id, slug)                            -- URL: /<thinker>/<work>
);

CREATE TABLE work_alias (
    work_id   bigint NOT NULL REFERENCES work (id) ON DELETE CASCADE,
    alias     text NOT NULL CHECK (alias <> ''),
    alias_key text NOT NULL CHECK (alias_key <> ''),
    PRIMARY KEY (work_id, alias_key)
);
CREATE INDEX work_alias_key_idx ON work_alias (alias_key);

-- ---------------------------------------------------------------------------
-- document: one stored file, exactly as downloaded. The bytes live in object storage
-- under their SHA-256; this row proves which bytes we hold and where they came from.
-- ---------------------------------------------------------------------------
CREATE TABLE document (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sha256       bytea NOT NULL UNIQUE CHECK (octet_length(sha256) = 32),
    byte_length  bigint NOT NULL CHECK (byte_length > 0),
    media_type   text NOT NULL CHECK (media_type <> ''), -- "text/plain; charset=utf-8"
    provider     text NOT NULL CHECK (provider ~ '^[a-z0-9]+(-[a-z0-9]+)*$'), -- gutenberg, wikisource
    provider_id  text NOT NULL CHECK (provider_id <> ''), -- "4583"
    source_url   text NOT NULL CHECK (source_url ~ '^https?://'),
    licence      text NOT NULL CHECK (licence <> ''),    -- "public-domain-us"
    retrieved_at timestamptz NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_id)
);

-- ---------------------------------------------------------------------------
-- edition: one work's text inside a document (a document can hold several works,
-- e.g. Anselm's Proslogion + Gaunilo's reply in one Gutenberg volume).
-- Byte offsets are into the document.
-- ---------------------------------------------------------------------------
CREATE TABLE edition (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    work_id        bigint NOT NULL REFERENCES work (id) ON DELETE RESTRICT,
    document_id    bigint NOT NULL REFERENCES document (id) ON DELETE RESTRICT,
    slug           text NOT NULL CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'), -- ?edition=gutenberg-4583
    title          text NOT NULL CHECK (title <> ''),    -- title as printed in this edition
    language       text NOT NULL CHECK (language ~ '^[a-z]{2,3}(-[A-Za-z0-9]+)*$'),
    translator     text CHECK (translator <> ''),
    published_year smallint,                             -- of this edition / translation
    start_byte     bigint NOT NULL CHECK (start_byte >= 0),
    end_byte       bigint NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CHECK (start_byte < end_byte),
    UNIQUE (document_id, work_id),
    UNIQUE (work_id, slug)
);
CREATE INDEX edition_document_idx ON edition (document_id);

-- ---------------------------------------------------------------------------
-- division: the edition's own structure as a tree (Part ▸ Chapter ▸ Section,
-- Question ▸ Article). Derived: rebuilt by the splitter, so it cascades.
-- ---------------------------------------------------------------------------
CREATE TABLE division (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    edition_id bigint NOT NULL REFERENCES edition (id) ON DELETE CASCADE,
    parent_id  bigint REFERENCES division (id) ON DELETE CASCADE, -- NULL = top level
    kind       text NOT NULL CHECK (kind ~ '^[a-z]+(-[a-z]+)*$'), -- part, chapter, section, …
    label      text NOT NULL CHECK (label <> ''),        -- "X", "3", "Introduction"
    ordinal    integer NOT NULL CHECK (ordinal >= 0),    -- order among siblings
    start_byte bigint NOT NULL CHECK (start_byte >= 0),
    end_byte   bigint NOT NULL,
    CHECK (start_byte < end_byte),
    -- NULLS NOT DISTINCT: two top-level divisions (parent NULL) can't share an ordinal either.
    UNIQUE NULLS NOT DISTINCT (edition_id, parent_id, ordinal)
);
CREATE INDEX division_parent_idx ON division (parent_id);

-- ---------------------------------------------------------------------------
-- passage: the smallest citable unit. Byte range = ground truth; `text` is a verified
-- copy of those bytes so search and display don't hit object storage.
-- ---------------------------------------------------------------------------
CREATE TABLE passage (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    edition_id        bigint NOT NULL REFERENCES edition (id) ON DELETE CASCADE,
    division_id       bigint REFERENCES division (id) ON DELETE CASCADE,
    ordinal           integer NOT NULL CHECK (ordinal >= 0), -- reading order within the edition
    locator           text NOT NULL                      -- work-level, shared across editions
                      CHECK (locator ~ '^[a-z0-9]+(-[a-z0-9]+)*(/[a-z0-9]+(-[a-z0-9]+)*)*$'),
    start_byte        bigint NOT NULL CHECK (start_byte >= 0),
    end_byte          bigint NOT NULL,
    text              text NOT NULL CHECK (text <> ''),
    content_sha256    bytea NOT NULL CHECK (octet_length(content_sha256) = 32),    -- exact bytes
    normalized_sha256 bytea NOT NULL CHECK (octet_length(normalized_sha256) = 32), -- for dedup
    created_at        timestamptz NOT NULL DEFAULT now(),
    CHECK (start_byte < end_byte),
    UNIQUE (edition_id, start_byte, end_byte),
    UNIQUE (edition_id, ordinal),
    UNIQUE (edition_id, locator)
);
CREATE INDEX passage_division_idx ON passage (division_id);
CREATE INDEX passage_locator_idx ON passage (locator);           -- /hume/dialogues/part-10/3
CREATE INDEX passage_normalized_idx ON passage (normalized_sha256); -- exact-duplicate lookup

-- ---------------------------------------------------------------------------
-- passage_label: the same passage in other reference systems (Kant A/B, Descartes AT,
-- Pascal Brunschvicg and Lafuma, Aquinas ST). A passage that crosses a page break can
-- carry two labels in the same scheme.
-- ---------------------------------------------------------------------------
CREATE TABLE passage_label (
    passage_id bigint NOT NULL REFERENCES passage (id) ON DELETE CASCADE,
    scheme     text NOT NULL CHECK (scheme ~ '^[a-z0-9]+(-[a-z0-9]+)*$'), -- kant-ab, descartes-at
    label      text NOT NULL CHECK (label <> ''),        -- "A598/B626", "AT VII 27"
    PRIMARY KEY (passage_id, scheme, label)
);
CREATE INDEX passage_label_lookup_idx ON passage_label (scheme, label);

-- ---------------------------------------------------------------------------
-- redirect: public URLs never break. When a slug changes, the old path forwards here.
-- ---------------------------------------------------------------------------
CREATE TABLE redirect (
    from_path  text PRIMARY KEY CHECK (from_path ~ '^/'),
    to_path    text NOT NULL CHECK (to_path ~ '^/'),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (from_path <> to_path)
);
