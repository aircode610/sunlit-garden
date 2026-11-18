//! Our strict XHTML reader: enough XML for well-formed files like Standard Ebooks', and an error
//! (not a guess) for anything else.

use sunlit_core::corpus::xhtml::{Token, tokenize};

#[test]
fn elements_attributes_and_decoded_text() {
    let src = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<!-- a comment -->
<section id="chapter-10" epub:type="chapter"><p>A &amp; B&#8217;s <em>real</em> case<br/></p></section>"#;

    let tokens = tokenize(src).unwrap();

    assert_eq!(
        tokens,
        vec![
            Token::Text("\n".into()),
            Token::Text("\n".into()),
            Token::Text("\n".into()),
            Token::Start {
                name: "section".into(),
                attrs: vec![
                    ("id".into(), "chapter-10".into()),
                    ("epub:type".into(), "chapter".into())
                ],
            },
            Token::Start {
                name: "p".into(),
                attrs: vec![]
            },
            Token::Text("A & B\u{2019}s ".into()),
            Token::Start {
                name: "em".into(),
                attrs: vec![]
            },
            Token::Text("real".into()),
            Token::End { name: "em".into() },
            Token::Text(" case".into()),
            Token::Start {
                name: "br".into(),
                attrs: vec![]
            },
            Token::End { name: "br".into() },
            Token::End { name: "p".into() },
            Token::End {
                name: "section".into()
            },
        ]
    );
}

#[test]
fn anything_not_well_formed_is_refused() {
    for (case, src) in [
        ("unknown entity", "<p>caf&eacute;</p>"),
        ("mismatched end tag", "<p><em>x</p></em>"),
        ("unclosed element", "<section><p>x</p>"),
        ("cdata", "<p><![CDATA[x]]></p>"),
        ("unquoted attribute", "<p id=x>y</p>"),
    ] {
        assert!(tokenize(src).is_err(), "{case} should be refused");
    }
}

#[test]
fn every_standard_ebooks_fixture_reads_cleanly() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/standard-ebooks/hume-enquiry");
    let mut paragraphs = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let tokens = tokenize(&std::fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        paragraphs += tokens
            .iter()
            .filter(|t| matches!(t, Token::Start { name, .. } if name == "p"))
            .count();
    }
    // The fixture is pinned, so the count is exact: 268 <p> (paragraphs and hgroup titles).
    assert_eq!(paragraphs, 268);
}
