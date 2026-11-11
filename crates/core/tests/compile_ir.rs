//! The compiler's intermediate representation and its first invariant: **coverage**.
//! Every non-whitespace byte of the body belongs to exactly one unit; nothing is silently
//! dropped or counted twice.

use sunlit_core::corpus::compile::{Compiled, Problem, Unit, UnitKind, check_coverage};

const FILE: &str = "HEADER\n\nPART 1\n\nFirst paragraph.\n\nSecond paragraph.\n\nFOOTER";
//                   0     7 8     14        24  ^ "First" starts at 16

fn unit(kind: UnitKind, text: &str) -> Unit {
    let start = FILE.find(text).unwrap();
    Unit { kind, range: start..start + text.len() }
}

fn body() -> std::ops::Range<usize> {
    let start = FILE.find("PART 1").unwrap();
    let end = FILE.find("Second paragraph.").unwrap() + "Second paragraph.".len();
    start..end
}

fn compiled(units: Vec<Unit>) -> Compiled {
    Compiled { body: body(), divisions: vec![], units }
}

#[test]
fn units_that_tile_the_body_pass() {
    let c = compiled(vec![
        unit(UnitKind::DivisionHeading, "PART 1"),
        unit(UnitKind::Text, "First paragraph."),
        unit(UnitKind::Text, "Second paragraph."),
    ]);

    assert_eq!(check_coverage(FILE.as_bytes(), &c), vec![]);
}

#[test]
fn text_not_covered_by_any_unit_is_reported() {
    let c = compiled(vec![unit(UnitKind::DivisionHeading, "PART 1"), unit(UnitKind::Text, "Second paragraph.")]);

    let problems = check_coverage(FILE.as_bytes(), &c);

    assert!(
        matches!(problems.as_slice(), [Problem::Uncovered { text, .. }] if text.contains("First paragraph.")),
        "{problems:?}"
    );
}

#[test]
fn overlapping_units_are_reported() {
    let first = unit(UnitKind::Text, "First paragraph.");
    let overlapping = Unit { kind: UnitKind::Text, range: first.range.start + 6..first.range.end };
    let c = compiled(vec![
        unit(UnitKind::DivisionHeading, "PART 1"),
        first,
        overlapping,
        unit(UnitKind::Text, "Second paragraph."),
    ]);

    let problems = check_coverage(FILE.as_bytes(), &c);

    assert!(problems.iter().any(|p| matches!(p, Problem::Overlap { .. })), "{problems:?}");
}

#[test]
fn units_outside_the_body_are_reported() {
    let c = compiled(vec![
        unit(UnitKind::Skipped("header"), "HEADER"),
        unit(UnitKind::DivisionHeading, "PART 1"),
        unit(UnitKind::Text, "First paragraph."),
        unit(UnitKind::Text, "Second paragraph."),
    ]);

    let problems = check_coverage(FILE.as_bytes(), &c);

    assert!(problems.iter().any(|p| matches!(p, Problem::OutsideBody { .. })), "{problems:?}");
}

#[test]
fn units_with_leading_or_trailing_whitespace_are_reported() {
    // Units must be trimmed, so their ranges cite exactly the words and nothing else.
    let start = FILE.find("First paragraph.").unwrap();
    let c = compiled(vec![
        unit(UnitKind::DivisionHeading, "PART 1"),
        Unit { kind: UnitKind::Text, range: start..start + "First paragraph.\n".len() },
        unit(UnitKind::Text, "Second paragraph."),
    ]);

    let problems = check_coverage(FILE.as_bytes(), &c);

    assert!(problems.iter().any(|p| matches!(p, Problem::Untrimmed { .. })), "{problems:?}");
}

#[test]
fn deliberately_skipped_text_counts_as_covered() {
    let c = compiled(vec![
        unit(UnitKind::DivisionHeading, "PART 1"),
        unit(UnitKind::Skipped("table of contents"), "First paragraph."),
        unit(UnitKind::Text, "Second paragraph."),
    ]);

    assert_eq!(check_coverage(FILE.as_bytes(), &c), vec![]);
}
