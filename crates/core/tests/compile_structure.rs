//! The structure pass on the real books. Expected numbers were measured independently from the
//! files (Python) before the pass was written; see the vault session note of 2026-11-11.

use sunlit_core::corpus::compile::structure::{
    Attribution, HeadingOverride, InsertionRule, Options, PlannedRole, Structure, compile_plain_text,
};
use sunlit_core::corpus::compile::UnitKind;
use sunlit_core::corpus::gutenberg::body_range;

const BERKELEY: &[u8] = include_bytes!("fixtures/gutenberg/pg4723.txt");
const HUME: &[u8] = include_bytes!("fixtures/gutenberg/pg4583.txt");

fn berkeley_options() -> Options {
    Options {
        headings: vec![HeadingOverride {
            text: "OF THE PRINCIPLES OF HUMAN KNOWLEDGE".into(),
            kind: "part".into(),
            label: "I".into(),
            slug: "part-1".into(),
        }],
        // Declared for edition #4723: its bracketed notes, markers and cross-references are the
        // editor's.
        insertions: vec![
            InsertionRule { prefix: "[Note".into(), by: Attribution::Editor },
            InsertionRule { prefix: "[Vide".into(), by: Attribution::Editor },
        ],
    }
}

fn hume_options() -> Options {
    Options {
        headings: vec![HeadingOverride {
            text: "PAMPHILUS TO HERMIPPUS".into(),
            kind: "letter".into(),
            label: "Pamphilus to Hermippus".into(),
            slug: "pamphilus-to-hermippus".into(),
        }],
        // Declared for edition #4583: bracketed text is Hume's own source references.
        insertions: vec![InsertionRule { prefix: "[".into(), by: Attribution::Author }],
    }
}

fn compile(file: &[u8], options: &Options) -> Structure {
    compile_plain_text(file, body_range(file).unwrap(), options)
}

fn text(file: &[u8], range: &std::ops::Range<usize>) -> String {
    String::from_utf8_lossy(&file[range.clone()]).into_owned()
}

fn locators_with_prefix(s: &Structure, prefix: &str, role: fn(&PlannedRole) -> bool) -> Vec<String> {
    s.passages.iter().filter(|p| p.locator.starts_with(prefix) && role(&p.role)).map(|p| p.locator.clone()).collect()
}

fn is_text(r: &PlannedRole) -> bool {
    matches!(r, PlannedRole::Text)
}

#[test]
fn berkeley_compiles_with_no_problems() {
    let s = compile(BERKELEY, &berkeley_options());
    assert_eq!(s.problems, Vec::<String>::new());
}

#[test]
fn berkeley_divisions_match_the_book_and_its_contents_list() {
    let s = compile(BERKELEY, &berkeley_options());

    let slugs: Vec<&str> = s.compiled.divisions.iter().map(|d| d.slug.as_str()).collect();
    assert_eq!(slugs, ["dedication", "preface", "introduction", "part-1"]);
    assert_eq!(s.toc, ["PREFACE", "INTRODUCTION", "OF THE PRINCIPLES OF HUMAN KNOWLEDGE"]);
}

#[test]
fn berkeley_has_exactly_25_introduction_and_156_part_one_sections() {
    let s = compile(BERKELEY, &berkeley_options());

    let intro = locators_with_prefix(&s, "introduction/", is_text);
    let part = locators_with_prefix(&s, "part-1/", is_text);
    assert_eq!(intro, (1..=25).map(|n| format!("introduction/{n}")).collect::<Vec<_>>());
    assert_eq!(part, (1..=156).map(|n| format!("part-1/{n}")).collect::<Vec<_>>());
}

#[test]
fn berkeley_part_one_section_3_is_the_candle_passage_without_heading_or_note() {
    let s = compile(BERKELEY, &berkeley_options());
    let find = |loc: &str| s.passages.iter().find(|p| p.locator == loc).unwrap_or_else(|| panic!("no {loc}"));

    let section = text(BERKELEY, &find("part-1/3").range);
    let heading = text(BERKELEY, &find("part-1/3/heading").range);
    let note = text(BERKELEY, &find("part-1/3/note-1").range);

    assert!(section.starts_with("That neither our thoughts"), "{section:?}");
    assert!(section.ends_with("thinking things which perceive them."), "{section:?}");
    assert!(section.contains("Their ESSE is PERCIPI"));
    assert_eq!(heading, "HOW FAR THE ASSENT OF THE VULGAR CONCEDED.--");
    assert_eq!(note, "[Note: First argument in support of the author's theory.]");
}

#[test]
fn berkeley_editor_material_is_attached_and_never_author_text() {
    let s = compile(BERKELEY, &berkeley_options());

    // Note *paragraphs* are EditorNote units; inline notes are Insertion units.
    let note_paragraphs = s.compiled.units.iter().filter(|u| u.kind == UnitKind::EditorNote).count();
    assert_eq!(note_paragraphs, 17, "10 in the Introduction, 7 in Part I");
    for p in &s.passages {
        match p.role {
            PlannedRole::EditorHeading { of }
            | PlannedRole::EditorNote { on: of }
            | PlannedRole::EditorMarker { in_: of }
            | PlannedRole::AuthorNote { on: of } => {
                let target = &s.passages[of];
                assert!(matches!(target.role, PlannedRole::Text), "{} attaches to non-text", p.locator);
                assert!(p.locator.starts_with(&format!("{}/", target.locator)), "{} attached to {}", p.locator, target.locator);
            }
            PlannedRole::Text => {
                let t = text(BERKELEY, &p.range);
                assert!(!t.starts_with("[Note"), "note stored as author text: {}", p.locator);
            }
        }
    }
}

#[test]
fn berkeley_two_part_and_bracketed_headings_are_whole() {
    let s = compile(BERKELEY, &berkeley_options());
    let heading = |loc: &str| text(BERKELEY, &s.passages.iter().find(|p| p.locator == loc).unwrap().range);

    assert_eq!(heading("part-1/34/heading"), "FIRST GENERAL OBJECTION.--ANSWER.--");
    assert_eq!(heading("part-1/35/heading"), "THE EXISTENCE OF MATTER, AS UNDERSTOOD BY PHILOSOPHERS,\r\nDENIED.[Vide sect. lxxxiv.]--");
    // Two segments; the first contains an inline note. (Checked against the raw text.)
    assert_eq!(heading("part-1/25/heading"), "THIRD ARGUMENT.[Note: Vide sect. iii. and vii.]--REFUTATION\r\nOF LOCKE.--");
    // Berkeley's own mixed-case sentences containing "--" are not headings.
    assert!(s.passages.iter().all(|p| p.locator != "introduction/5/heading"));
    assert!(s.passages.iter().all(|p| p.locator != "part-1/38/heading"));
}

#[test]
fn berkeley_multi_paragraph_sections_are_one_passage() {
    let s = compile(BERKELEY, &berkeley_options());
    let section = |loc: &str| text(BERKELEY, &s.passages.iter().find(|p| p.locator == loc).unwrap().range);

    assert!(section("part-1/34").contains("FIRST, then, it will be objected"));
    assert!(section("part-1/86").contains("And first as to ideas or unthinking things."));
}

#[test]
fn berkeley_signature_line_is_flagged_not_made_a_division() {
    let s = compile(BERKELEY, &berkeley_options());
    assert!(s.warnings.iter().any(|w| w.contains("GEORGE BERKELEY")), "{:?}", s.warnings);
    assert!(s.compiled.divisions.iter().all(|d| d.slug != "george-berkeley"));
}

#[test]
fn hume_compiles_into_the_letter_and_twelve_parts() {
    let s = compile(HUME, &hume_options());

    assert_eq!(s.problems, Vec::<String>::new());
    let slugs: Vec<&str> = s.compiled.divisions.iter().map(|d| d.slug.as_str()).collect();
    let mut expected = vec!["pamphilus-to-hermippus".to_string()];
    expected.extend((1..=12).map(|n| format!("part-{n}")));
    assert_eq!(slugs, expected);
}

#[test]
fn hume_paragraph_counts_per_part_match_the_book() {
    let s = compile(HUME, &hume_options());
    let counts: Vec<usize> = (1..=12).map(|n| locators_with_prefix(&s, &format!("part-{n}/"), is_text).len()).collect();

    assert_eq!(counts, [22, 28, 13, 14, 17, 13, 18, 12, 11, 41, 21, 35]);
    assert_eq!(locators_with_prefix(&s, "pamphilus-to-hermippus/", is_text).len(), 6);
    assert!(!text(HUME, &s.passages.iter().find(|p| p.locator == "part-10/1").unwrap().range).is_empty());
}

#[test]
fn hume_without_its_override_is_refused_not_stored_wrong() {
    // Without knowing "PAMPHILUS TO HERMIPPUS" is a heading, the letter would vanish into the
    // title page. The pass must notice and refuse.
    let s = compile(HUME, &Options::default());
    assert!(s.problems.iter().any(|p| p.contains("before the first heading")), "{:?}", s.problems);
}

#[test]
fn an_override_that_matches_nothing_is_a_problem() {
    let mut options = hume_options();
    options.headings[0].text = "PAMPHILUS TO HERMIPUS".into(); // typo
    let s = compile(HUME, &options);
    assert!(s.problems.iter().any(|p| p.contains("matches no line")), "{:?}", s.problems);
}

fn count(s: &Structure, role: fn(&PlannedRole) -> bool) -> usize {
    s.passages.iter().filter(|p| role(&p.role)).count()
}

#[test]
fn berkeley_insertions_inside_his_sentences_are_the_editors() {
    let s = compile(BERKELEY, &berkeley_options());

    // Measured independently from the file: 17 bare markers, 2 of them inside editor headings;
    // 5 inline notes, 1 inside a heading; 4 cross-references, 1 inside a heading.
    assert_eq!(count(&s, |r| matches!(r, PlannedRole::EditorMarker { .. })), 15);
    assert_eq!(count(&s, |r| matches!(r, PlannedRole::EditorNote { .. })), 17 + 4 + 3, "17 note paragraphs + 4 inline notes + 3 cross-references");
    assert_eq!(count(&s, |r| matches!(r, PlannedRole::AuthorNote { .. })), 0);
}

#[test]
fn berkeley_author_words_exclude_the_editors_inline_note() {
    let s = compile(BERKELEY, &berkeley_options());
    let i = s.passages.iter().position(|p| p.locator == "part-1/22").unwrap();

    let raw = text(BERKELEY, &s.passages[i].range);
    let words = s.author_words(BERKELEY, i);

    assert!(raw.contains("--Ed.]"), "the bytes still hold the editor's note (evidence)");
    assert!(!words.contains("--Ed."), "but it is not Berkeley's words: {words}");
    assert!(!words.contains('['));
}

#[test]
fn berkeley_candle_passage_words_drop_the_note_marker() {
    let s = compile(BERKELEY, &berkeley_options());
    let i = s.passages.iter().position(|p| p.locator == "part-1/3").unwrap();

    let words = s.author_words(BERKELEY, i);

    assert!(words.contains("some other spirit actually does perceive it. There was an odour"), "{words}");
    assert!(s.passages.iter().any(|p| p.locator == "part-1/3/marker-1"));
}

#[test]
fn hume_brackets_are_his_own_references() {
    let s = compile(HUME, &hume_options());

    assert_eq!(count(&s, |r| matches!(r, PlannedRole::AuthorNote { .. })), 10);
    assert_eq!(count(&s, |r| matches!(r, PlannedRole::EditorNote { .. })), 0);
    let i = s.passages.iter().position(|p| text(HUME, &p.range).contains("[Recherche de la Verite")).unwrap();
    assert!(s.author_words(HUME, i).contains("[Recherche de la Verite. Liv. 3. Chap.9]"), "author notes stay in the author's words");
}

#[test]
fn hume_empty_brackets_are_flagged_as_a_transcription_artefact() {
    let s = compile(HUME, &hume_options());

    assert_eq!(count(&s, |r| matches!(r, PlannedRole::EditorMarker { .. })), 1);
    assert!(s.warnings.iter().any(|w| w.contains("empty brackets")), "{:?}", s.warnings);
    let i = s.passages.iter().position(|p| text(HUME, &p.range).contains("it is said[]Dr. Clarke")).unwrap();
    assert!(s.author_words(HUME, i).contains("it is said Dr. Clarke"));
}

#[test]
fn undeclared_bracket_styles_send_the_edition_to_review() {
    let mut options = berkeley_options();
    options.insertions.retain(|r| r.prefix != "[Vide");

    let s = compile(BERKELEY, &options);

    assert!(s.problems.iter().any(|p| p.contains("not declared") && p.contains("[Vide sect.")), "{:?}", s.problems);
}
