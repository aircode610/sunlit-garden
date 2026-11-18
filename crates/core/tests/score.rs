//! The compiler scorer on the Hume Enquiry test pair: Standard Ebooks is the answer key.

use sunlit_core::corpus::score::{DivisionMark, standard_ebooks_gold};

const DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/standard-ebooks/hume-enquiry"
);

fn spine() -> Vec<(String, String)> {
    let opf = std::fs::read_to_string(format!("{DIR}/content.opf")).unwrap();
    // The reading order is the content.opf spine; the fixture keeps only the text chapters.
    let mut files = Vec::new();
    for idref in opf
        .split("<itemref idref=\"")
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
    {
        if let Ok(text) = std::fs::read_to_string(format!("{DIR}/{idref}")) {
            files.push((idref.to_owned(), text));
        }
    }
    files
}

#[test]
fn the_answer_key_has_the_books_sections_and_parts() {
    let gold = standard_ebooks_gold(&spine()).unwrap();

    let top: Vec<&str> = gold
        .divisions
        .iter()
        .filter(|d| d.depth == 1)
        .map(|d| d.label.as_str())
        .collect();
    assert_eq!(
        top,
        (1..=12)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    // Parts: Sections IV, V, VII, VIII, X have two, XII has three.
    let parts = |section: &str| {
        let i = gold
            .divisions
            .iter()
            .position(|d| d.depth == 1 && d.label == section)
            .unwrap();
        gold.divisions[i + 1..]
            .iter()
            .take_while(|d| d.depth == 2)
            .count()
    };
    let counts: Vec<usize> = (1..=12).map(|n| parts(&n.to_string())).collect();
    assert_eq!(counts, [0, 0, 0, 2, 2, 0, 2, 2, 0, 2, 0, 3]);

    // Section X, Part I opens "There is, in Dr. Tillotson's writings, an argument…".
    let x1 = gold
        .divisions
        .iter()
        .position(|d| d.depth == 1 && d.label == "10")
        .unwrap()
        + 1;
    let DivisionMark { at, .. } = gold.divisions[x1];
    assert_eq!(
        gold.words[at..at + 6],
        ["there", "is", "in", "dr", "tillotsons", "writings"]
    );
    assert!(
        gold.boundaries.contains(&at),
        "a passage starts where the Part starts"
    );
}

const GUTENBERG_9662: &[u8] = include_bytes!("fixtures/gutenberg/pg9662.txt");

fn compiled_9662() -> sunlit_core::corpus::compile::structure::Structure {
    use sunlit_core::corpus::compile::structure::{Options, compile_plain_text};
    let body = sunlit_core::corpus::gutenberg::body_range(GUTENBERG_9662).unwrap();
    compile_plain_text(GUTENBERG_9662, body, &Options::default())
}

#[test]
fn a_compiled_edition_becomes_the_same_shape() {
    use sunlit_core::corpus::compile::structure::PlannedRole;
    use sunlit_core::corpus::score::{segment, words};
    let structure = compiled_9662();

    let seg = segment(GUTENBERG_9662, &structure);

    let texts: Vec<usize> = (0..structure.passages.len())
        .filter(|&i| structure.passages[i].role == PlannedRole::Text)
        .collect();
    let expected: Vec<String> = texts
        .iter()
        .flat_map(|&i| words(&structure.author_words(GUTENBERG_9662, i)).collect::<Vec<_>>())
        .collect();
    assert_eq!(seg.words, expected);
    let non_empty = texts
        .iter()
        .filter(|&&i| {
            words(&structure.author_words(GUTENBERG_9662, i))
                .next()
                .is_some()
        })
        .count();
    assert_eq!(seg.boundaries.len(), non_empty);
    assert!(
        seg.boundaries.windows(2).all(|w| w[0] < w[1]),
        "boundaries ascend"
    );
    let nodes = |ds: &[sunlit_core::corpus::compile::DivisionNode]| -> usize {
        fn count(ds: &[sunlit_core::corpus::compile::DivisionNode]) -> usize {
            ds.iter().map(|d| 1 + count(&d.children)).sum()
        }
        count(ds)
    };
    assert_eq!(seg.divisions.len(), nodes(&structure.compiled.divisions));
    // Today's compiler doesn't nest Parts inside Sections: every division is top level.
    assert!(seg.divisions.iter().all(|d| d.depth == 1));
    assert_eq!(seg.divisions[0].label, "1");
}
