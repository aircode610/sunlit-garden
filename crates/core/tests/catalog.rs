//! Gutenberg's catalogue (`pg_catalog.csv`): parsing and querying, on an excerpt of the real
//! file chosen for its awkward rows (multi-line titles, several authors, two languages).

use sunlit_core::corpus::catalog::{CatalogError, Query, parse};

const EXCERPT: &[u8] = include_bytes!("fixtures/gutenberg/pg_catalog_excerpt.csv");

#[test]
fn every_row_parses_into_a_typed_entry() {
    let entries = parse(EXCERPT).unwrap();

    assert_eq!(entries.len(), 11);
    let enquiry = entries.iter().find(|e| e.id == 9662).unwrap();
    assert_eq!(enquiry.title, "An Enquiry Concerning Human Understanding");
    assert_eq!(enquiry.authors.len(), 2);
    assert_eq!(enquiry.authors[0], "Hume, David, 1711-1776");
    assert!(
        enquiry.authors[1].ends_with("[Editor]"),
        "{:?}",
        enquiry.authors
    );
    assert_eq!(enquiry.locc, ["B"]);
    let summa = entries.iter().find(|e| e.id == 17611).unwrap();
    assert!(summa.title.contains('\n'), "multi-line titles survive");
    let luther = entries.iter().find(|e| e.id == 417).unwrap();
    assert_eq!(luther.languages, ["de", "en"]);
    let bill = entries.iter().find(|e| e.id == 2).unwrap();
    assert_eq!(bill.locc, ["JK", "KF"]);
}

#[test]
fn a_changed_header_is_refused_rather_than_guessed() {
    let renamed = String::from_utf8(EXCERPT.to_vec()).unwrap().replacen(
        "Authors,Subjects",
        "Subjects,Authors",
        1,
    );

    let result = parse(renamed.as_bytes());

    assert!(
        matches!(&result, Err(CatalogError::UnexpectedHeader(h)) if h[5] == "Subjects"),
        "{result:?}"
    );
}

fn ids(query: &Query) -> Vec<u32> {
    let mut ids: Vec<u32> = parse(EXCERPT)
        .unwrap()
        .iter()
        .filter(|e| query.matches(e))
        .map(|e| e.id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn a_query_selects_english_philosophy_texts_by_class() {
    let query = Query {
        languages: vec!["en".into()],
        locc: vec!["B".into()],
        ..Query::default()
    };

    // B = philosophy + religion (BS, BX… start with B too): the Bible (10, BS), Aquinas (17611,
    // BX) and Luther (417, de+en, BX) are in; the Pi dataset and the US documents are out.
    assert_eq!(ids(&query), [10, 59, 417, 4583, 4723, 8909, 9662, 17611]);
}

#[test]
fn author_matches_case_insensitively_anywhere_in_the_authors() {
    let query = Query {
        author: Some("hume, david".into()),
        ..Query::default()
    };

    assert_eq!(ids(&query), [4583, 9662]);
}

#[test]
fn excluded_classes_drop_a_book_only_if_it_has_no_other_class() {
    let query = Query {
        languages: vec!["en".into()],
        locc: vec!["B".into()],
        exclude_locc: vec!["BS".into(), "BX".into()],
        ..Query::default()
    };

    // The Bible (BS), Aquinas (BX) and Luther (BX) go; B-only books stay.
    assert_eq!(ids(&query), [59, 4583, 4723, 8909, 9662]);
    // A book that is philosophy *and* Catholic theology keeps its philosophy class.
    let mut both = parse(EXCERPT)
        .unwrap()
        .into_iter()
        .find(|e| e.id == 17611)
        .unwrap();
    both.locc = vec!["B".into(), "BX".into()];
    assert!(query.matches(&both));
    // An unclassified book isn't "excluded": only a class filter can drop it.
    let mut unclassified = both.clone();
    unclassified.locc.clear();
    let only_exclusions = Query {
        exclude_locc: vec!["BX".into()],
        ..Query::default()
    };
    assert!(only_exclusions.matches(&unclassified));
}
