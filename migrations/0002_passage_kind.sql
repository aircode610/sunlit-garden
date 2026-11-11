-- Migration 2: whose words is a passage?
--
-- Editions mix the author's text with an editor's: added section headings
-- ("HOW FAR THE ASSENT OF THE VULGAR CONCEDED.--" in Berkeley's Principles) and notes
-- ("[Note: First argument in support of the author's theory.]"). Those must never be cited as
-- the thinker's words, so each passage records its kind, and editorial passages point at the
-- author passage they belong to.
--
--   kind = 'text'            the author's own words           attached_to IS NULL
--   kind = 'editor-heading'  heading added by an editor       attached_to = its section
--   kind = 'editor-note'     an editor's note or cross-ref    attached_to = the passage it annotates
--   kind = 'editor-marker'   an editor's bare note marker     attached_to = the passage it sits in
--                            ("[Note.]", "[Note 1.]")
--   kind = 'author-note'     the author's own note/reference  attached_to = the passage it annotates
--
-- Notes and markers can be **insertions**: bracketed text printed inside an author passage
-- (Berkeley's "[Note: … --Ed.]", Hume's "[Recherche de la Verite. Liv. 3. Chap.9]"). An
-- insertion's range lies inside its host's range: the host keeps its contiguous bytes (the
-- evidence), and the author's words are the host minus its non-author insertions. Who wrote a
-- bracket style is declared per edition (corpus/sources.toml), never guessed.
--
-- The pairing is a CHECK, so the database itself refuses an unattached note or an attached
-- author passage. Passages are derived data, so deleting a section removes its editorial
-- passages with it (CASCADE).

ALTER TABLE passage
    ADD COLUMN kind text NOT NULL DEFAULT 'text'
        CHECK (kind IN ('text', 'editor-heading', 'editor-note', 'editor-marker', 'author-note')),
    ADD COLUMN attached_to bigint REFERENCES passage (id) ON DELETE CASCADE,
    ADD CONSTRAINT passage_attachment_matches_kind
        CHECK ((kind = 'text') = (attached_to IS NULL)),
    ADD CONSTRAINT passage_not_attached_to_itself
        CHECK (attached_to <> id);

-- The default only exists so the column could be added to existing rows; new rows must say
-- what they are.
ALTER TABLE passage ALTER COLUMN kind DROP DEFAULT;

CREATE INDEX passage_attached_to_idx ON passage (attached_to);
