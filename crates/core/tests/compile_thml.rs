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
    let s = compile_thml(file, "iii", &Options::default());
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
