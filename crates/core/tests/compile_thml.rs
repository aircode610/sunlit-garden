//! The ThML front-end (CCEL's XML): divisions from `divN` elements, passages from body `<p>`s,
//! byte ranges into the untouched file (markup kept), author words with tags removed.

use sunlit_core::corpus::compile::check_coverage;
use sunlit_core::corpus::compile::structure::{Options, PlannedRole};
use sunlit_core::corpus::compile::thml::compile_thml;

const VOLUME: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ThML><ThML.body>
<div1 title="Introduction" id="ii"><p>By the translator.</p></div1>
<div1 title="Proslogium" id="iii">
<div2 title="Preface" id="iii.i">
<h3>PREFACE.</h3>
<p class="PContinue">After I had published a little work.</p>
</div2>
<div2 title="Chapter II." id="iii.iii">
<h3 id="iii.iii-p0.1">CHAPTER II.</h3>
<p class="SectionInfo">Truly there is a God, although the fool hath said.</p>
<p class="PContinue">AND so, Lord, do <i>thou</i> &amp; give.<note n="1">A note by Deane.</note> Amen.</p>
<pb n="8" id="iii.iii-Page_8" />
<p class="PContinue">For, it is one thing.</p>
</div2>
</div1>
</ThML.body></ThML>"#;

#[test]
fn a_thml_work_is_its_divisions_and_paragraphs() {
    let file = VOLUME.as_bytes();
    let s = compile_thml(file, "iii", &[], &Options::default());
    assert!(s.problems.is_empty(), "{:?}", s.problems);
    assert!(
        check_coverage(file, &s.compiled).is_empty(),
        "every byte accounted for"
    );

    let d = &s.compiled.divisions;
    let slugs: Vec<&str> = d.iter().map(|x| x.slug.as_str()).collect();
    assert_eq!(slugs, ["preface", "chapter-2"]);
    assert_eq!(
        d[1].title.as_deref(),
        Some("Truly there is a God, although the fool hath said")
    );

    let plan: Vec<(&str, PlannedRole, &str)> = s
        .passages
        .iter()
        .map(|p| (p.locator.as_str(), p.role, &VOLUME[p.range.clone()]))
        .collect();
    assert_eq!(
        plan,
        [
            (
                "preface/1",
                PlannedRole::Text,
                "After I had published a little work."
            ),
            (
                "chapter-2/1",
                PlannedRole::Text,
                r#"AND so, Lord, do <i>thou</i> &amp; give.<note n="1">A note by Deane.</note> Amen."#
            ),
            (
                "chapter-2/1/note-1",
                PlannedRole::EditorNote { on: 1 },
                r#"<note n="1">A note by Deane.</note>"#
            ),
            ("chapter-2/2", PlannedRole::Text, "For, it is one thing."),
        ]
    );
    // What search and hashes see: tags gone, entities decoded, the editor's note removed.
    assert_eq!(
        s.author_words(file, 1),
        "AND so, Lord, do thou & give. Amen."
    );
}

#[test]
fn an_empty_self_closing_paragraph_is_no_passage() {
    let file = br#"<ThML><div1 title="Proslogium" id="iii"><div2 title="Chapter I." id="iii.ii"><p/><p>Up now.</p></div2></div1></ThML>"#;
    let s = compile_thml(file, "iii", &[], &Options::default());
    assert!(s.problems.is_empty(), "{:?}", s.problems);
    assert_eq!(s.passages.len(), 1);
}

/// Gaunilo's reply as CCEL prints it: no sub-divisions, a centred title block, then sections
/// numbered by the text ("1. IF one doubts…") with continuation paragraphs. Declared
/// `passages = "numbered-sections"`, each section is one passage with no division, cited "1".
#[test]
fn an_undivided_work_in_numbered_sections() {
    use sunlit_core::corpus::compile::structure::PassageUnit;
    let src = r#"<ThML><div2 title="In Behalf of the Fool" id="v.i">
<p style="text-align:center" id="v.i-p1"><cite>IN BEHALF OF THE FOOL.</cite></p>
<p style="text-align:center" id="v.i-p3">BY <name>GAUNILON</name>, A MONK OF MARMOUTIER.</p>
<p style="text-indent:.2in" id="v.i-p4" />
<p class="PFirst">1. IF one doubts or denies the existence of a being.</p>
<p class="PContinue">The existence of this being is proved.</p>
<p class="PFirst">2. To this assertion, I reply.</p>
</div2></ThML>"#;
    let options = Options {
        passages: PassageUnit::NumberedSections,
        ..Options::default()
    };
    let s = compile_thml(src.as_bytes(), "v.i", &[], &options);
    assert!(s.problems.is_empty(), "{:?}", s.problems);
    assert!(s.compiled.divisions.is_empty());
    let plan: Vec<(&str, Option<usize>, String)> = s
        .passages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                p.locator.as_str(),
                p.division,
                s.author_words(src.as_bytes(), i),
            )
        })
        .collect();
    assert_eq!(
        plan,
        [
            ("1", None, "IF one doubts or denies the existence of a being. The existence of this being is proved.".to_owned()),
            ("2", None, "To this assertion, I reply.".to_owned()),
        ]
    );
}

/// A paragraph holding only a page break and a no-break space (ANF 7 has hundreds) is no
/// passage, and Unicode whitespace between units is whitespace.
#[test]
fn a_paragraph_without_words_is_no_passage() {
    let file = "<ThML><div2 title=\"The Divine Institutes\" id=\"iii.ii\"><div3 title=\"Book I.\" id=\"b1\"><p><pb n=\"9\"/> \u{a0}</p><p>Men of great and excelling genius.</p></div3></div2></ThML>";
    let s = compile_thml(file.as_bytes(), "iii.ii", &[], &Options::default());
    assert!(s.problems.is_empty(), "{:?}", s.problems);
    let texts: Vec<&str> = s.passages.iter().map(|p| &file[p.range.clone()]).collect();
    assert_eq!(texts, ["Men of great and excelling genius."]);
}

/// CCEL nests the Epitome of the Divine Institutes (a work of its own) inside the Institutes'
/// element: `except` leaves it out of this edition.
#[test]
fn an_excepted_element_is_another_work() {
    let file = br#"<ThML><div2 title="The Divine Institutes" id="iii.ii"><div3 title="Book I." id="b1"><p>Men of great genius.</p></div3><div3 title="The Epitome of the Divine Institutes" id="iii.ii.viii"><p>Although the books.</p></div3></div2></ThML>"#;
    let s = compile_thml(
        file,
        "iii.ii",
        &["iii.ii.viii".to_owned()],
        &Options::default(),
    );
    assert!(s.problems.is_empty(), "{:?}", s.problems);
    let slugs: Vec<&str> = s
        .compiled
        .divisions
        .iter()
        .map(|d| d.slug.as_str())
        .collect();
    assert_eq!(slugs, ["book-1"]);
    assert_eq!(s.passages.len(), 1);
}

/// Verse quoted in the text (Lactantius quoting Virgil) is a passage like a paragraph.
#[test]
fn a_verse_block_is_a_passage() {
    let file = br#"<ThML><div2 title="The Divine Institutes" id="iii.ii"><div3 title="Book I." id="b1"><p>As the poet says:</p><verse><l class="t1">Know first, the heaven, the earth,</l><l class="t1">The moon's pale orb.</l></verse></div3></div2></ThML>"#;
    let s = compile_thml(file, "iii.ii", &[], &Options::default());
    assert!(s.problems.is_empty(), "{:?}", s.problems);
    assert_eq!(s.passages.len(), 2);
    assert_eq!(
        s.author_words(file, 1),
        "Know first, the heaven, the earth, The moon's pale orb."
    );
}
