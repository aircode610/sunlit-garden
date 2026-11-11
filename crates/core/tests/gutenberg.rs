//! Finding the book inside a Project Gutenberg plain-text file (between its licence header
//! and footer). Fixtures use CRLF line endings, like the real files.

use sunlit_core::corpus::gutenberg::{GutenbergError, body_range};

const FILE: &str = "The Project Gutenberg eBook of Dialogues\r\n\
    Title: Dialogues Concerning Natural Religion\r\n\
    \r\n\
    *** START OF THE PROJECT GUTENBERG EBOOK DIALOGUES CONCERNING NATURAL RELIGION ***\r\n\
    \r\n\
    \r\n\
    PAMPHILUS TO HERMIPPUS\r\n\
    It has been remarked, my Hermippus...\r\n\
    \r\n\
    *** END OF THE PROJECT GUTENBERG EBOOK DIALOGUES CONCERNING NATURAL RELIGION ***\r\n\
    Updated editions will replace the previous one...\r\n";

#[test]
fn body_is_the_text_between_the_markers_without_surrounding_blank_lines() {
    let range = body_range(FILE.as_bytes()).unwrap();

    assert_eq!(&FILE[range], "PAMPHILUS TO HERMIPPUS\r\nIt has been remarked, my Hermippus...");
}

#[test]
fn older_this_project_markers_are_accepted() {
    let old = FILE.replace("OF THE PROJECT", "OF THIS PROJECT");

    let range = body_range(old.as_bytes()).unwrap();

    assert!(old[range].starts_with("PAMPHILUS"));
}

#[test]
fn a_missing_marker_is_refused() {
    let no_end = FILE.replace("*** END OF", "*** FIN DE");
    assert!(matches!(body_range(no_end.as_bytes()), Err(GutenbergError::Markers { start: 1, end: 0 })));
}

#[test]
fn a_duplicated_marker_is_refused() {
    let twice = format!("{FILE}*** START OF THE PROJECT GUTENBERG EBOOK AGAIN ***\r\n");
    assert!(matches!(body_range(twice.as_bytes()), Err(GutenbergError::Markers { start: 2, end: 1 })));
}

#[test]
fn end_before_start_is_refused() {
    let swapped = "*** END OF THE PROJECT GUTENBERG EBOOK X ***\r\ntext\r\n*** START OF THE PROJECT GUTENBERG EBOOK X ***\r\n";
    assert!(matches!(body_range(swapped.as_bytes()), Err(GutenbergError::EndBeforeStart)));
}

#[test]
fn a_leading_transcriber_credit_paragraph_is_not_part_of_the_book() {
    // Real Gutenberg files often open the body with a volunteer credit, not the work's text.
    let credited = FILE.replace(
        "\r\n\r\n\r\nPAMPHILUS",
        "\r\n\r\nProduced by Col Choat.  HTML version by Al Haines.\r\n\r\n\r\nPAMPHILUS",
    );

    let range = body_range(credited.as_bytes()).unwrap();

    assert!(credited[range].starts_with("PAMPHILUS TO HERMIPPUS"), "credit must be skipped");
}

#[test]
fn credit_skipping_stops_at_the_first_blank_line_even_with_mixed_line_endings() {
    // CRLF credit paragraph, then a later LF-only blank line: must stop at the CRLF one.
    let mixed = "*** START OF THE PROJECT GUTENBERG EBOOK X ***\r\n\
        Produced by A Volunteer.\r\n\r\n\
        BOOK ONE\r\nfirst line\n\nsecond paragraph\r\n\
        *** END OF THE PROJECT GUTENBERG EBOOK X ***\r\n";

    let range = body_range(mixed.as_bytes()).unwrap();

    assert!(mixed[range].starts_with("BOOK ONE"), "got {:?}", &mixed[body_range(mixed.as_bytes()).unwrap()]);
}
