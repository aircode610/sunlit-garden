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
