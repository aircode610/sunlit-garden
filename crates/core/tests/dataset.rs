//! The ADU dataset's resolver and validator, on the real Berkeley and Hume texts.

use sunlit_core::corpus::compile::compile_edition;
use sunlit_core::corpus::compile::structure::Structure;
use sunlit_core::corpus::manifest::Manifest;
use sunlit_core::dataset::{Record, check, parse_jsonl, resolve};

const SOURCES: &str = include_str!("../../../corpus/sources.toml");
const BERKELEY: &[u8] = include_bytes!("fixtures/gutenberg/pg4723.txt");
const HUME: &[u8] = include_bytes!("fixtures/gutenberg/pg4583.txt");

fn structure(slug: &str, file: &[u8]) -> Structure {
    let m = Manifest::parse(SOURCES).unwrap();
    let e = m.documents.iter().flat_map(|d| &d.editions).find(|e| e.slug.as_str() == slug).unwrap();
    compile_edition(file, e).unwrap()
}

const HEADER: &str = r#"{"kind":"edition","slug":"gutenberg-4723","scope":"pilot"}"#;

fn records(lines: &[&str]) -> Vec<Record> {
    parse_jsonl(&lines.join("\n")).unwrap()
}

fn unit(id: &str, loc: &str, quote: &str, t: &str) -> String {
    format!(r#"{{"kind":"unit","id":"{id}","locator":"{loc}","quote":"{quote}","type":"{t}","voice":"author","stance":"asserted","annotator":"agent","status":"draft"}}"#)
}

const VERDICT_3: &str = r#"{"kind":"passage","locator":"part-1/3","verdict":"argumentative","annotator":"agent","status":"draft"}"#;

/// Resolve then check; returns (resolve problems, check problems).
fn run(file: &[u8], s: &Structure, lines: &[&str]) -> (Vec<String>, Vec<String>, Vec<Record>) {
    let mut r = records(lines);
    let resolved = resolve(&mut r, file, s);
    let checked = check(&r, file, s).problems;
    (resolved, checked, r)
}

#[test]
fn a_quote_across_line_breaks_resolves_to_the_exact_bytes() {
    let s = structure("gutenberg-4723", BERKELEY);
    // Lines 792-793 of the file: "...That neither our thoughts,\r\nnor passions, nor ideas..."
    let u = unit("part-1/3#u1", "part-1/3", "That neither our thoughts, nor passions, nor ideas formed by the imagination, exist WITHOUT the mind", "claim");

    let (resolved, checked, r) = run(BERKELEY, &s, &[HEADER, VERDICT_3, &u]);

    assert_eq!((resolved, checked), (vec![], vec![]));
    let Record::Unit(u) = &r[2] else { panic!() };
    let bytes = &BERKELEY[u.start.unwrap()..u.end.unwrap()];
    assert_eq!(String::from_utf8_lossy(bytes).split_whitespace().collect::<Vec<_>>().join(" "), u.quote);
    assert!(bytes.windows(2).any(|w| w == b"\r\n"), "this quote really spans a line break in the file");
}

#[test]
fn missing_ambiguous_and_editorial_quotes_are_refused() {
    let s = structure("gutenberg-4723", BERKELEY);
    let missing = unit("part-1/3#u1", "part-1/3", "matter exists independently", "claim");
    let ambiguous = unit("part-1/3#u2", "part-1/3", "it was", "claim");
    let across_marker = unit("part-1/3#u3", "part-1/3", "spirit actually does perceive it.[Note.] There was an odour", "example");

    let (resolved, _, _) = run(BERKELEY, &s, &[HEADER, VERDICT_3, &missing, &ambiguous, &across_marker]);

    assert!(resolved.iter().any(|p| p.contains("u1") && p.contains("not found")), "{resolved:?}");
    assert!(resolved.iter().any(|p| p.contains("u2") && p.contains("times")), "{resolved:?}");
    assert!(resolved.iter().any(|p| p.contains("u3") && p.contains("isn't the author's words")), "{resolved:?}");
}

#[test]
fn every_structural_rule_is_enforced() {
    let s = structure("gutenberg-4723", BERKELEY);
    let a = unit("part-1/3#u1", "part-1/3", "That neither our thoughts, nor passions, nor ideas formed by the imagination, exist WITHOUT the mind", "claim");
    let overlapping = unit("part-1/3#u2", "part-1/3", "ideas formed by the imagination", "premise");
    let unknown_rel = r#"{"kind":"relation","id":"part-1/3#r1","from":"part-1/3#u9","to":"part-1/3#u1","type":"support","annotator":"agent","status":"draft"}"#;
    let no_reason = r#"{"kind":"passage","locator":"part-1/4","verdict":"non-argumentative","annotator":"agent","status":"draft"}"#;
    let empty_arg = r#"{"kind":"passage","locator":"part-1/5","verdict":"argumentative","annotator":"agent","status":"draft"}"#;

    let (_, checked, _) = run(BERKELEY, &s, &[HEADER, VERDICT_3, &a, &overlapping, unknown_rel, no_reason, empty_arg]);

    for expected in ["overlap", "endpoint part-1/3#u9", "part-1/4: non-argumentative needs a reason", "part-1/5: argumentative but has no units"] {
        assert!(checked.iter().any(|p| p.contains(expected)), "missing {expected:?} in {checked:?}");
    }
}

#[test]
fn tampered_offsets_are_caught() {
    let s = structure("gutenberg-4723", BERKELEY);
    let u = unit("part-1/3#u1", "part-1/3", "cannot exist otherwise than IN a mind perceiving them", "claim");
    let (_, _, mut r) = run(BERKELEY, &s, &[HEADER, VERDICT_3, &u]);
    let Record::Unit(u) = &mut r[2] else { panic!() };
    u.start = Some(u.start.unwrap() + 1); // someone edits the file by hand

    let checked = check(&r, BERKELEY, &s).problems;

    assert!(checked.iter().any(|p| p.contains("no longer matches")), "{checked:?}");
}

#[test]
fn full_scope_requires_every_author_passage() {
    let s = structure("gutenberg-4723", BERKELEY);
    let header = r#"{"kind":"edition","slug":"gutenberg-4723","scope":"full"}"#;

    let report = check(&records(&[header]), BERKELEY, &s);

    assert_eq!(report.author_passages, 186);
    assert_eq!(report.problems.iter().filter(|p| p.contains("no verdict")).count(), 186);
}

#[test]
fn in_a_dialogue_only_the_cast_may_speak() {
    let s = structure("gutenberg-4583", HUME);
    let header = r#"{"kind":"edition","slug":"gutenberg-4583","scope":"pilot","speakers":["philo","cleanthes","demea","pamphilus"]}"#;
    let verdict = r#"{"kind":"passage","locator":"part-1/2","verdict":"argumentative","annotator":"agent","status":"draft"}"#;
    let as_author = unit("part-1/2#u1", "part-1/2", "my education", "claim");

    let (_, checked, _) = run(HUME, &s, &[header, verdict, &as_author]);
    assert!(checked.iter().any(|p| p.contains("voice \"author\" not allowed")), "{checked:?}");

    let as_speaker = as_author.replace("\"voice\":\"author\"", "\"voice\":\"speaker:demea\"");
    let (_, checked, _) = run(HUME, &s, &[header, verdict, &as_speaker]);
    assert!(!checked.iter().any(|p| p.contains("voice")), "{checked:?}");
}

#[test]
fn malformed_records_are_rejected_with_line_numbers() {
    let typo = r#"{"kind":"unit","id":"x#u1","locator":"x","quote":"y","type":"claim","voice":"author","stance":"asserted","annotator":"agent","status":"draft","confidence":0.9}"#;
    let bad_enum = r#"{"kind":"relation","id":"x#r1","from":"a","to":"b","type":"supports","annotator":"agent","status":"draft"}"#;

    let errors = parse_jsonl(&[HEADER, typo, bad_enum].join("\n")).unwrap_err();

    assert!(errors.iter().any(|e| e.starts_with("line 2") && e.contains("confidence")), "{errors:?}");
    assert!(errors.iter().any(|e| e.starts_with("line 3") && e.contains("supports")), "{errors:?}");
}
