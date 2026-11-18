//! The structure pass on the real books. Expected numbers were measured independently from the
//! files (Python) before the pass was written; see the vault session note of 2026-11-11.

use sunlit_core::corpus::compile::UnitKind;
use sunlit_core::corpus::compile::structure::{
    Attribution, HeadingOverride, InsertionRule, Options, PlannedRole, Structure,
    compile_plain_text,
};
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
            InsertionRule {
                prefix: "[Note".into(),
                by: Attribution::Editor,
            },
            InsertionRule {
                prefix: "[Vide".into(),
                by: Attribution::Editor,
            },
        ],
        expect: None,
        ..Options::default()
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
        insertions: vec![InsertionRule {
            prefix: "[".into(),
            by: Attribution::Author,
        }],
        expect: None,
        ..Options::default()
    }
}

fn compile(file: &[u8], options: &Options) -> Structure {
    compile_plain_text(file, body_range(file).unwrap(), options)
}

fn text(file: &[u8], range: &std::ops::Range<usize>) -> String {
    String::from_utf8_lossy(&file[range.clone()]).into_owned()
}

fn locators_with_prefix(
    s: &Structure,
    prefix: &str,
    role: fn(&PlannedRole) -> bool,
) -> Vec<String> {
    s.passages
        .iter()
        .filter(|p| p.locator.starts_with(prefix) && role(&p.role))
        .map(|p| p.locator.clone())
        .collect()
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

    let slugs: Vec<&str> = s
        .compiled
        .divisions
        .iter()
        .map(|d| d.slug.as_str())
        .collect();
    assert_eq!(slugs, ["dedication", "preface", "introduction", "part-1"]);
    assert_eq!(
        s.toc,
        [
            "PREFACE",
            "INTRODUCTION",
            "OF THE PRINCIPLES OF HUMAN KNOWLEDGE"
        ]
    );
}

#[test]
fn berkeley_has_exactly_25_introduction_and_156_part_one_sections() {
    let s = compile(BERKELEY, &berkeley_options());

    let intro = locators_with_prefix(&s, "introduction/", is_text);
    let part = locators_with_prefix(&s, "part-1/", is_text);
    assert_eq!(
        intro,
        (1..=25)
            .map(|n| format!("introduction/{n}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        part,
        (1..=156).map(|n| format!("part-1/{n}")).collect::<Vec<_>>()
    );
}

#[test]
fn berkeley_part_one_section_3_is_the_candle_passage_without_heading_or_note() {
    let s = compile(BERKELEY, &berkeley_options());
    let find = |loc: &str| {
        s.passages
            .iter()
            .find(|p| p.locator == loc)
            .unwrap_or_else(|| panic!("no {loc}"))
    };

    let section = text(BERKELEY, &find("part-1/3").range);
    let heading = text(BERKELEY, &find("part-1/3/heading").range);
    let note = text(BERKELEY, &find("part-1/3/note-1").range);

    assert!(
        section.starts_with("That neither our thoughts"),
        "{section:?}"
    );
    assert!(
        section.ends_with("thinking things which perceive them."),
        "{section:?}"
    );
    assert!(section.contains("Their ESSE is PERCIPI"));
    assert_eq!(heading, "HOW FAR THE ASSENT OF THE VULGAR CONCEDED.--");
    assert_eq!(
        note,
        "[Note: First argument in support of the author's theory.]"
    );
}

#[test]
fn berkeley_editor_material_is_attached_and_never_author_text() {
    let s = compile(BERKELEY, &berkeley_options());

    // Note *paragraphs* are EditorNote units; inline notes are Insertion units.
    let note_paragraphs = s
        .compiled
        .units
        .iter()
        .filter(|u| u.kind == UnitKind::EditorNote)
        .count();
    assert_eq!(note_paragraphs, 17, "10 in the Introduction, 7 in Part I");
    for p in &s.passages {
        match p.role {
            PlannedRole::EditorHeading { of }
            | PlannedRole::EditorNote { on: of }
            | PlannedRole::EditorMarker { in_: of }
            | PlannedRole::AuthorNote { on: of } => {
                let target = &s.passages[of];
                assert!(
                    matches!(target.role, PlannedRole::Text),
                    "{} attaches to non-text",
                    p.locator
                );
                assert!(
                    p.locator.starts_with(&format!("{}/", target.locator)),
                    "{} attached to {}",
                    p.locator,
                    target.locator
                );
            }
            PlannedRole::Text => {
                let t = text(BERKELEY, &p.range);
                assert!(
                    !t.starts_with("[Note"),
                    "note stored as author text: {}",
                    p.locator
                );
            }
        }
    }
}

#[test]
fn berkeley_two_part_and_bracketed_headings_are_whole() {
    let s = compile(BERKELEY, &berkeley_options());
    let heading = |loc: &str| {
        text(
            BERKELEY,
            &s.passages.iter().find(|p| p.locator == loc).unwrap().range,
        )
    };

    assert_eq!(
        heading("part-1/34/heading"),
        "FIRST GENERAL OBJECTION.--ANSWER.--"
    );
    assert_eq!(
        heading("part-1/35/heading"),
        "THE EXISTENCE OF MATTER, AS UNDERSTOOD BY PHILOSOPHERS,\r\nDENIED.[Vide sect. lxxxiv.]--"
    );
    // Two segments; the first contains an inline note. (Checked against the raw text.)
    assert_eq!(
        heading("part-1/25/heading"),
        "THIRD ARGUMENT.[Note: Vide sect. iii. and vii.]--REFUTATION\r\nOF LOCKE.--"
    );
    // Berkeley's own mixed-case sentences containing "--" are not headings.
    assert!(
        s.passages
            .iter()
            .all(|p| p.locator != "introduction/5/heading")
    );
    assert!(s.passages.iter().all(|p| p.locator != "part-1/38/heading"));
}

#[test]
fn berkeley_multi_paragraph_sections_are_one_passage() {
    let s = compile(BERKELEY, &berkeley_options());
    let section = |loc: &str| {
        text(
            BERKELEY,
            &s.passages.iter().find(|p| p.locator == loc).unwrap().range,
        )
    };

    assert!(section("part-1/34").contains("FIRST, then, it will be objected"));
    assert!(section("part-1/86").contains("And first as to ideas or unthinking things."));
}

#[test]
fn berkeley_signature_line_is_flagged_not_made_a_division() {
    let s = compile(BERKELEY, &berkeley_options());
    assert!(
        s.warnings.iter().any(|w| w.contains("GEORGE BERKELEY")),
        "{:?}",
        s.warnings
    );
    assert!(
        s.compiled
            .divisions
            .iter()
            .all(|d| d.slug != "george-berkeley")
    );
}

#[test]
fn hume_compiles_into_the_letter_and_twelve_parts() {
    let s = compile(HUME, &hume_options());

    assert_eq!(s.problems, Vec::<String>::new());
    let slugs: Vec<&str> = s
        .compiled
        .divisions
        .iter()
        .map(|d| d.slug.as_str())
        .collect();
    let mut expected = vec!["pamphilus-to-hermippus".to_string()];
    expected.extend((1..=12).map(|n| format!("part-{n}")));
    assert_eq!(slugs, expected);
}

#[test]
fn hume_paragraph_counts_per_part_match_the_book() {
    let s = compile(HUME, &hume_options());
    let counts: Vec<usize> = (1..=12)
        .map(|n| locators_with_prefix(&s, &format!("part-{n}/"), is_text).len())
        .collect();

    assert_eq!(counts, [22, 28, 13, 14, 17, 13, 18, 12, 11, 41, 21, 35]);
    assert_eq!(
        locators_with_prefix(&s, "pamphilus-to-hermippus/", is_text).len(),
        6
    );
    assert!(
        !text(
            HUME,
            &s.passages
                .iter()
                .find(|p| p.locator == "part-10/1")
                .unwrap()
                .range
        )
        .is_empty()
    );
}

#[test]
fn hume_without_its_override_is_refused_not_stored_wrong() {
    // Without knowing "PAMPHILUS TO HERMIPPUS" is a heading, the letter would vanish into the
    // title page. The pass must notice and refuse.
    let s = compile(HUME, &Options::default());
    assert!(
        s.problems
            .iter()
            .any(|p| p.contains("before the first heading")),
        "{:?}",
        s.problems
    );
}

#[test]
fn an_override_that_matches_nothing_is_a_problem() {
    let mut options = hume_options();
    options.headings[0].text = "PAMPHILUS TO HERMIPUS".into(); // typo
    let s = compile(HUME, &options);
    assert!(
        s.problems.iter().any(|p| p.contains("matches no line")),
        "{:?}",
        s.problems
    );
}

fn count(s: &Structure, role: fn(&PlannedRole) -> bool) -> usize {
    s.passages.iter().filter(|p| role(&p.role)).count()
}

#[test]
fn berkeley_insertions_inside_his_sentences_are_the_editors() {
    let s = compile(BERKELEY, &berkeley_options());

    // Measured independently from the file: 17 bare markers, 2 of them inside editor headings;
    // 5 inline notes, 1 inside a heading; 4 cross-references, 1 inside a heading.
    assert_eq!(
        count(&s, |r| matches!(r, PlannedRole::EditorMarker { .. })),
        15
    );
    assert_eq!(
        count(&s, |r| matches!(r, PlannedRole::EditorNote { .. })),
        17 + 4 + 3,
        "17 note paragraphs + 4 inline notes + 3 cross-references"
    );
    assert_eq!(
        count(&s, |r| matches!(r, PlannedRole::AuthorNote { .. })),
        0
    );
}

#[test]
fn berkeley_author_words_exclude_the_editors_inline_note() {
    let s = compile(BERKELEY, &berkeley_options());
    let i = s
        .passages
        .iter()
        .position(|p| p.locator == "part-1/22")
        .unwrap();

    let raw = text(BERKELEY, &s.passages[i].range);
    let words = s.author_words(BERKELEY, i);

    assert!(
        raw.contains("--Ed.]"),
        "the bytes still hold the editor's note (evidence)"
    );
    assert!(
        !words.contains("--Ed."),
        "but it is not Berkeley's words: {words}"
    );
    assert!(!words.contains('['));
}

#[test]
fn berkeley_candle_passage_words_drop_the_note_marker() {
    let s = compile(BERKELEY, &berkeley_options());
    let i = s
        .passages
        .iter()
        .position(|p| p.locator == "part-1/3")
        .unwrap();

    let words = s.author_words(BERKELEY, i);

    assert!(
        words.contains("some other spirit actually does perceive it. There was an odour"),
        "{words}"
    );
    assert!(s.passages.iter().any(|p| p.locator == "part-1/3/marker-1"));
}

#[test]
fn hume_brackets_are_his_own_references() {
    let s = compile(HUME, &hume_options());

    assert_eq!(
        count(&s, |r| matches!(r, PlannedRole::AuthorNote { .. })),
        10
    );
    assert_eq!(
        count(&s, |r| matches!(r, PlannedRole::EditorNote { .. })),
        0
    );
    let i = s
        .passages
        .iter()
        .position(|p| text(HUME, &p.range).contains("[Recherche de la Verite"))
        .unwrap();
    assert!(
        s.author_words(HUME, i)
            .contains("[Recherche de la Verite. Liv. 3. Chap.9]"),
        "author notes stay in the author's words"
    );
}

#[test]
fn hume_empty_brackets_are_flagged_as_a_transcription_artefact() {
    let s = compile(HUME, &hume_options());

    assert_eq!(
        count(&s, |r| matches!(r, PlannedRole::EditorMarker { .. })),
        1
    );
    assert!(
        s.warnings.iter().any(|w| w.contains("empty brackets")),
        "{:?}",
        s.warnings
    );
    let i = s
        .passages
        .iter()
        .position(|p| text(HUME, &p.range).contains("it is said[]Dr. Clarke"))
        .unwrap();
    assert!(s.author_words(HUME, i).contains("it is said Dr. Clarke"));
}

#[test]
fn undeclared_bracket_styles_send_the_edition_to_review() {
    let mut options = berkeley_options();
    options.insertions.retain(|r| r.prefix != "[Vide");

    let s = compile(BERKELEY, &options);

    assert!(
        s.problems
            .iter()
            .any(|p| p.contains("not declared") && p.contains("[Vide sect.")),
        "{:?}",
        s.problems
    );
}

#[test]
fn declared_expectations_are_hard_checks() {
    use sunlit_core::corpus::compile::structure::Expect;
    let mut options = berkeley_options();
    options.expect = Some(Expect {
        divisions: Some(vec![
            "dedication".into(),
            "preface".into(),
            "introduction".into(),
            "part-1".into(),
        ]),
        text_passages: [
            ("introduction".to_string(), 25),
            ("part-1".to_string(), 156),
        ]
        .into(),
    });
    assert_eq!(compile(BERKELEY, &options).problems, Vec::<String>::new());

    options
        .expect
        .as_mut()
        .unwrap()
        .text_passages
        .insert("part-1".into(), 157);
    let problems = compile(BERKELEY, &options).problems;
    assert!(
        problems
            .iter()
            .any(|p| p.contains("part-1") && p.contains("156") && p.contains("157")),
        "{problems:?}"
    );
}

/// Hume's Enquiry (Gutenberg #9662): Sections I–XII, some with Parts I, II (III). The Parts nest
/// inside their Section, and their passages are addressed through it.
#[test]
fn parts_nest_inside_the_section_they_follow() {
    let file: &[u8] = include_bytes!("fixtures/gutenberg/pg9662.txt");
    let body = sunlit_core::corpus::gutenberg::body_range(file).unwrap();
    let s = compile_plain_text(file, body, &Options::default());
    let divs = &s.compiled.divisions;

    let section = |n: &str| {
        divs.iter()
            .position(|d| d.kind == "section" && d.slug == format!("section-{n}"))
            .unwrap()
    };
    let children = |p: usize| -> Vec<&str> {
        divs.iter()
            .filter(|d| d.parent == Some(p))
            .map(|d| d.slug.as_str())
            .collect()
    };
    assert_eq!(children(section("4")), ["part-1", "part-2"]);
    assert_eq!(children(section("10")), ["part-1", "part-2"]);
    assert_eq!(children(section("12")), ["part-1", "part-2", "part-3"]);
    assert!(children(section("6")).is_empty());
    assert!(
        divs.iter()
            .filter(|d| d.kind == "section")
            .all(|d| d.parent.is_none()),
        "sections are top level"
    );

    // Passages are addressed through their Section, so the locators are unique again.
    assert!(
        s.passages
            .iter()
            .any(|p| p.locator == "section-10/part-1/1"),
        "{:?}",
        s.passages
            .iter()
            .map(|p| &p.locator)
            .take(5)
            .collect::<Vec<_>>()
    );
    assert!(
        !s.problems
            .iter()
            .any(|p| p.contains("two divisions with locator")),
        "{:?}",
        s.problems
    );
}

/// A back-of-book index is the publisher's apparatus, not the author's text: like a contents
/// list it's skipped (still covered), never passages.
#[test]
fn a_back_of_book_index_is_skipped_not_text() {
    let file: &[u8] = include_bytes!("fixtures/gutenberg/pg9662.txt");
    let s = compile_plain_text(file, body_range(file).unwrap(), &Options::default());

    assert!(
        !s.passages.iter().any(|p| p.locator.starts_with("index")),
        "index entries became passages"
    );
    assert!(!s.compiled.divisions.iter().any(|d| d.slug == "index"));
    let skipped = s
        .compiled
        .units
        .iter()
        .filter(|u| u.kind == UnitKind::Skipped("index"))
        .count();
    assert!(skipped > 100, "{skipped} index paragraphs skipped");
    assert!(
        !s.problems.iter().any(|p| p.contains("in no unit")),
        "everything still covered"
    );
}

/// Footnotes printed as indented paragraphs ("    [7] Nothing is more useful…", plus any
/// indented paragraphs that continue it) are notes on the passage with the marker "[7]", not text.
#[test]
fn footnote_paragraphs_are_notes_on_the_passage_with_their_marker() {
    use sunlit_core::corpus::compile::structure::PlannedRole;
    let file: &[u8] = include_bytes!("fixtures/gutenberg/pg9662.txt");
    let s = compile_plain_text(file, body_range(file).unwrap(), &Options::default());
    let texts: Vec<String> = (0..s.passages.len())
        .filter(|&i| s.passages[i].role == PlannedRole::Text)
        .map(|i| s.author_words(file, i))
        .collect();

    let footnote_like = texts
        .iter()
        .filter(|t| {
            t.trim_start().starts_with('[') && t[1..].starts_with(|c: char| c.is_ascii_digit())
        })
        .count();
    assert_eq!(
        footnote_like, 0,
        "a text passage starts with a footnote number"
    );
    assert!(
        !texts
            .iter()
            .any(|t| t.starts_with("The same distinction between reason and experience")),
        "footnote 7's second paragraph is still text"
    );
    assert!(
        !s.problems.iter().any(|p| p.contains("in no unit")),
        "everything still covered"
    );
    // Without a declaration of who wrote the notes, the edition goes to review (as for brackets).
    assert!(
        s.problems
            .iter()
            .any(|p| p.contains("note style not declared"))
    );
}

/// A numbered division's title ("SECTION X." then "OF MIRACLES.") is part of its heading, not a
/// passage. A footnote on a title can't be attached yet (notes attach to passages): a problem.
#[test]
fn a_numbered_divisions_title_is_part_of_its_heading() {
    use sunlit_core::corpus::compile::structure::{PassageUnit, PlannedRole};
    let file: &[u8] = include_bytes!("fixtures/gutenberg/pg9662.txt");
    // The Enquiry is cited by paragraph (EHU 10.1); its printed numbers are Selby-Bigge's.
    let options = Options {
        passages: PassageUnit::Paragraphs,
        ..Options::default()
    };
    let s = compile_plain_text(file, body_range(file).unwrap(), &options);
    let section = |n: &str| {
        s.compiled
            .divisions
            .iter()
            .find(|d| d.slug == format!("section-{n}"))
            .unwrap()
    };

    assert_eq!(section("10").title.as_deref(), Some("OF MIRACLES"));
    assert_eq!(section("6").title.as_deref(), Some("OF PROBABILITY"));
    let texts = (0..s.passages.len())
        .filter(|&i| s.passages[i].role == PlannedRole::Text)
        .count();
    assert_eq!(
        texts, 256,
        "one text passage per paragraph of the book, as in the answer key"
    );
    assert!(
        s.problems
            .iter()
            .any(|p| p.contains("footnote on a heading")),
        "{:?}",
        s.problems
            .iter()
            .filter(|p| !p.contains("not declared"))
            .collect::<Vec<_>>()
    );
    assert!(
        !s.problems.iter().any(|p| p.contains("in no unit")),
        "everything still covered"
    );
}

#[test]
fn the_passage_unit_can_be_declared_per_edition() {
    use sunlit_core::corpus::compile::structure::PassageUnit;
    use sunlit_core::corpus::manifest::Manifest;
    let toml = include_str!("../../../corpus/sources.toml").replace(
        "slug = \"gutenberg-4583\"\n",
        "slug = \"gutenberg-4583\"\npassages = \"paragraphs\"\n",
    );
    let m = Manifest::parse(&toml).unwrap();
    let e = |slug: &str| {
        m.documents
            .iter()
            .flat_map(|d| &d.editions)
            .find(|e| e.slug.as_str() == slug)
            .unwrap()
            .compile
            .passages
    };
    assert_eq!(e("gutenberg-4583"), PassageUnit::Paragraphs);
    assert_eq!(e("gutenberg-4723"), PassageUnit::Auto, "default");
}

/// Headings as Smith's Moral Sentiments (Gutenberg #67363) prints them: "CONTENTS." with a full
/// stop, abbreviated "CHAP. I.", and the title on the next lines of the heading paragraph
/// ("PART I." / "Of the PROPRIETY of ACTION."). Its contents list also contains heading-like
/// lines ("PART I."): the list ends where its first heading appears again, in the body.
#[test]
fn headings_with_a_full_stop_an_abbreviation_or_a_title_underneath() {
    use sunlit_core::corpus::compile::structure::PassageUnit;
    let file: &[u8] = include_bytes!("fixtures/gutenberg/pg67363.txt");
    let options = Options {
        passages: PassageUnit::Paragraphs,
        ..Options::default()
    };
    let s = compile_plain_text(file, body_range(file).unwrap(), &options);
    let d = &s.compiled.divisions;
    let find = |kind: &str, label: &str, parent: Option<usize>| {
        d.iter()
            .position(|x| x.kind == kind && x.label == label && x.parent == parent)
            .unwrap_or_else(|| panic!("{kind} {label} under {parent:?}"))
    };

    assert!(!s.toc.is_empty(), "the contents list is found");
    let parts: Vec<&str> = d
        .iter()
        .filter(|x| x.kind == "part" && x.parent.is_none())
        .map(|x| x.label.as_str())
        .collect();
    // The 1777 text: six Parts (Smith added a seventh in 1790).
    assert_eq!(parts, ["I", "II", "III", "IV", "V", "VI"]);
    let part1 = find("part", "I", None);
    assert_eq!(
        d[part1].title.as_deref(),
        Some("Of the PROPRIETY of ACTION")
    );
    // Part I > Section I > Chap. I.
    let sect1 = find("section", "I", Some(part1));
    let chap1 = find("chapter", "I", Some(sect1));
    assert_eq!(d[chap1].title.as_deref(), Some("Of SYMPATHY"));
    // "Consisting of three Sections." is heading material, not a passage, and doesn't replace
    // the title printed in the heading ("CONSISTING OF THREE SECTIONS." under Part II).
    let part2 = find("part", "II", None);
    let title2 = d[part2].title.as_deref().unwrap();
    assert!(title2.starts_with("Of MERIT and DEMERIT"), "{title2}");
    assert!(
        !s.passages
            .iter()
            .any(|p| file[p.range.clone()].starts_with(b"Consisting"))
    );
    // A Part or Section holding only sub-divisions is not a division without text.
    assert!(
        !s.problems.iter().any(|p| p.contains("has no text")),
        "{:?}",
        s.problems
    );
    // "INTRODUCTION." right after "SECTION II." opens inside it; its chapters follow it there.
    let sect2 = find("section", "II", Some(part1));
    find("introduction", "Introduction", Some(sect2));
    find("chapter", "I", Some(sect2));
    // Titles of up to six lines (Part II, Section I, Chap. III).
    let p2s1 = find("section", "I", Some(part2));
    let long = d[find("chapter", "III", Some(p2s1))]
        .title
        .as_deref()
        .unwrap();
    assert!(
        long.starts_with("That where there is no approbation"),
        "{long}"
    );
    assert!(
        long.ends_with("the resentment of him who suffers it"),
        "{long}"
    );
}

/// A title printed in the heading paragraph is the title: a following line in capitals with
/// text after it is not taken as the title instead.
#[test]
fn a_title_printed_in_the_heading_is_not_replaced() {
    use sunlit_core::corpus::compile::structure::PassageUnit;
    let file = b"CHAPTER I.\nOf Sympathy.\n\nTHE ARGUMENT.\n\nHow selfish soever man may be supposed.\n\nCHAPTER II.\nOf Pleasure.\n\nWe may judge of the propriety.\n";
    let options = Options {
        passages: PassageUnit::Paragraphs,
        ..Options::default()
    };
    let s = compile_plain_text(file, 0..file.len(), &options);
    let titles: Vec<Option<&str>> = s
        .compiled
        .divisions
        .iter()
        .map(|d| d.title.as_deref())
        .collect();
    assert_eq!(titles, [Some("Of Sympathy"), Some("Of Pleasure")]);
}

/// Headings as Hobbes' Leviathan (Gutenberg #3207) prints them: the title on the heading line
/// ("CHAPTER I. OF SENSE", "CHAPTER XL OF THE RIGHTS …" without a full stop), wrapping over a
/// blank line into a paragraph in capitals; chapters numbered through the Parts; a contents list
/// headed "CONTENTS OF THE CHAPTERS" whose entries include "INTRODUCTION", and the body's
/// "THE INTRODUCTION".
#[test]
fn titles_on_the_heading_line_and_chapters_numbered_through_the_parts() {
    use sunlit_core::corpus::compile::structure::PassageUnit;
    let file: &[u8] = include_bytes!("fixtures/gutenberg/pg3207.txt");
    let options = Options {
        passages: PassageUnit::Paragraphs,
        ..Options::default()
    };
    let s = compile_plain_text(file, body_range(file).unwrap(), &options);
    let d = &s.compiled.divisions;
    let find = |kind: &str, label: &str| {
        d.iter()
            .position(|x| x.kind == kind && x.label == label)
            .unwrap_or_else(|| panic!("{kind} {label}"))
    };

    assert!(!s.toc.is_empty(), "the contents list is found");
    assert_eq!(d.iter().filter(|x| x.kind == "introduction").count(), 1);
    // The contents list ends where the body's "THE INTRODUCTION" repeats its "INTRODUCTION".
    let intro = find("introduction", "Introduction");
    let first = s.passages.iter().find(|p| p.division == intro).unwrap();
    assert!(file[first.range.clone()].starts_with(b"Nature (the art whereby God"));
    let parts: Vec<(&str, Option<&str>)> = d
        .iter()
        .filter(|x| x.kind == "part")
        .map(|x| (x.label.as_str(), x.title.as_deref()))
        .collect();
    assert_eq!(parts[0], ("I", Some("OF MAN")));
    assert_eq!(parts.len(), 4);
    let chap1 = find("chapter", "I");
    assert_eq!(d[chap1].title.as_deref(), Some("OF SENSE"));
    assert_eq!(d[chap1].parent, Some(find("part", "I")));
    // Chapter XVII opens Part II: chapters were inside Parts before, so it goes inside Part II.
    assert_eq!(d[find("chapter", "XVII")].parent, Some(find("part", "II")));
    assert_eq!(d[find("chapter", "XLVII")].parent, Some(find("part", "IV")));
    let xl = d[find("chapter", "XL")].title.as_deref().unwrap();
    assert!(
        xl.starts_with("OF THE RIGHTS OF THE KINGDOME OF GOD"),
        "{xl}"
    );
    let vi = d[find("chapter", "VI")].title.as_deref().unwrap();
    assert!(
        vi.starts_with("OF THE INTERIOUR BEGINNINGS OF VOLUNTARY MOTIONS COMMONLY CALLED"),
        "{vi}"
    );
    assert!(vi.ends_with("BY WHICH THEY ARE EXPRESSED"), "{vi}");
    assert!(
        !s.problems
            .iter()
            .any(|p| p.contains("has no text") || p.contains("two divisions")),
        "{:?}",
        s.problems
    );
}

/// A title on the heading line needs the line in capitals: prose that happens to start with a
/// heading word and a number is not a heading.
#[test]
fn prose_starting_with_part_and_a_number_is_not_a_heading() {
    use sunlit_core::corpus::compile::structure::PassageUnit;
    let file = b"CHAPTER I. OF SENSE\n\nConcerning the thoughts of man.\n\nPart II of this work treats of the commonwealth.\n";
    let options = Options {
        passages: PassageUnit::Paragraphs,
        ..Options::default()
    };
    let s = compile_plain_text(file, 0..file.len(), &options);
    let d = &s.compiled.divisions;
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].title.as_deref(), Some("OF SENSE"));
}
