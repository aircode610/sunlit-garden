//! The paragraph pass: the fallback that works on any plain text. Tested on the real
//! Berkeley (#4723) and Hume (#4583) files, kept byte-for-byte as imported.

use sunlit_core::Sha256;
use sunlit_core::corpus::compile::paragraphs::paragraphs;
use sunlit_core::corpus::compile::{Compiled, Unit, UnitKind, check_coverage};
use sunlit_core::corpus::gutenberg::body_range;

pub const BERKELEY: &[u8] = include_bytes!("fixtures/gutenberg/pg4723.txt");
pub const HUME: &[u8] = include_bytes!("fixtures/gutenberg/pg4583.txt");

#[test]
fn fixtures_are_exactly_the_pinned_files() {
    // Same pins as corpus/sources.toml: the tests run on the bytes we actually imported.
    assert_eq!(
        Sha256::of(BERKELEY).to_hex(),
        "ba577d0e3437d92faeb6fedc01715e4703ee8ae3f94b4ccb0eb5c772508c3f93"
    );
    assert_eq!(
        Sha256::of(HUME).to_hex(),
        "a6e4592ad61467f6e2722207bb07450aca8e81835e67d07401ac47026f36af42"
    );
}

#[test]
fn paragraphs_are_blocks_separated_by_blank_lines() {
    let file = b"One line.\r\nStill one.\r\n\r\n\r\nTwo.\r\n   \r\nThree\r\n";

    let ranges = paragraphs(file, 0..file.len());

    let texts: Vec<&str> = ranges
        .iter()
        .map(|r| std::str::from_utf8(&file[r.clone()]).unwrap())
        .collect();
    assert_eq!(texts, ["One line.\r\nStill one.", "Two.", "Three"]);
}

fn check_real(file: &[u8], first: &str, last: &str) {
    let body = body_range(file).unwrap();
    let ranges = paragraphs(file, body.clone());
    let units = ranges
        .iter()
        .map(|r| Unit {
            kind: UnitKind::Text,
            range: r.clone(),
        })
        .collect();
    let compiled = Compiled {
        body,
        divisions: vec![],
        units,
    };

    assert_eq!(
        check_coverage(file, &compiled),
        vec![],
        "paragraphs must tile the whole body"
    );
    let text = |r: &std::ops::Range<usize>| String::from_utf8_lossy(&file[r.clone()]).into_owned();
    assert!(
        text(&ranges[0]).starts_with(first),
        "first: {:?}",
        text(&ranges[0])
    );
    assert!(
        text(ranges.last().unwrap()).ends_with(last),
        "last: {:?}",
        text(ranges.last().unwrap())
    );
    for r in &ranges {
        let t = text(r);
        assert!(
            !t.contains("\r\n\r\n") && !t.contains("\n\n"),
            "a paragraph contains a blank line: {t:?}"
        );
    }
}

#[test]
fn berkeley_is_fully_covered_by_paragraphs() {
    check_real(
        BERKELEY,
        "A Treatise Concerning the Principles of Human Knowledge",
        "highest perfection of human nature.",
    );
}

#[test]
fn hume_is_fully_covered_by_paragraphs() {
    check_real(
        HUME,
        "Dialogues Concerning Natural Religion",
        "approach still nearer to the truth.",
    );
}
