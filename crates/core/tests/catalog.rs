//! Gutenberg's catalogue (`pg_catalog.csv`): parsing and querying, on an excerpt of the real
//! file chosen for its awkward rows (multi-line titles, several authors, two languages).

use sunlit_core::corpus::catalog::{CatalogError, parse};

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
