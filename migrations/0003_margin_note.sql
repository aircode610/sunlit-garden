-- Migration 3: the author's marginal notes.
--
-- Some editions print the author's side-notes as paragraphs of their own: Gutenberg's
-- Leviathan has 669 of Hobbes' ("Apparitions Or Visions"), each before the paragraph it
-- glosses. They are the author's words but not running text, so they are their own kind,
-- attached to that paragraph (outside its bytes, unlike an insertion):
--
--   kind = 'margin-note'     the author's marginal note       attached_to = the passage it glosses
--
-- Which editions print them, and how, is declared per edition (corpus/sources.toml).
-- The CHECK from migration 2 is replaced, not edited: Postgres can't alter a CHECK in place.
-- Added NOT VALID (no scan under the table's exclusive lock), then validated, which scans
-- while reads and writes go on. The new check only allows more, so validation can't fail.

ALTER TABLE passage
    DROP CONSTRAINT passage_kind_check,
    ADD CONSTRAINT passage_kind_check
        CHECK (kind IN ('text', 'editor-heading', 'editor-note', 'editor-marker', 'author-note',
                        'margin-note')) NOT VALID;

ALTER TABLE passage VALIDATE CONSTRAINT passage_kind_check;
