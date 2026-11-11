-- Migration 2: whose words is a passage?
--
-- Editions mix the author's text with an editor's: added section headings
-- ("HOW FAR THE ASSENT OF THE VULGAR CONCEDED.--" in Berkeley's Principles) and notes
-- ("[Note: First argument in support of the author's theory.]"). Those must never be cited as
-- the thinker's words, so each passage records its kind, and editorial passages point at the
-- author passage they belong to.
--
--   kind = 'text'            the author's own words      attached_to IS NULL
--   kind = 'editor-heading'  heading added by an editor  attached_to = its section
--   kind = 'editor-note'     an editor's note            attached_to = the passage it annotates
--
-- The pairing is a CHECK, so the database itself refuses an unattached note or an attached
-- author passage. Passages are derived data, so deleting a section removes its editorial
-- passages with it (CASCADE).

ALTER TABLE passage
    ADD COLUMN kind text NOT NULL DEFAULT 'text'
        CHECK (kind IN ('text', 'editor-heading', 'editor-note')),
    ADD COLUMN attached_to bigint REFERENCES passage (id) ON DELETE CASCADE,
    ADD CONSTRAINT passage_attachment_matches_kind
        CHECK ((kind = 'text') = (attached_to IS NULL)),
    ADD CONSTRAINT passage_not_attached_to_itself
        CHECK (attached_to <> id);

-- The default only exists so the column could be added to existing rows; new rows must say
-- what they are.
ALTER TABLE passage ALTER COLUMN kind DROP DEFAULT;

CREATE INDEX passage_attached_to_idx ON passage (attached_to);
