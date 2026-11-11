//! The hand-curated source list (`corpus/sources.toml`): parsing and validation.
//! Everything is checked before anything is imported, and every problem is reported at once.

use sunlit_core::corpus::manifest::{Manifest, ManifestError, RangeSpec};
use sunlit_core::{Sha256, Slug};

const HUME: &str = r#"
[[thinker]]
slug = "david-hume"
name = "David Hume"
wikidata = "Q37160"
born = 1711
died = 1776

[[work]]
thinker = "david-hume"
slug = "dialogues-concerning-natural-religion"
title = "Dialogues Concerning Natural Religion"
wikidata = "Q965564"
first_year = 1779
original_language = "en"

[[document]]
provider = "gutenberg"
provider_id = "4583"
url = "https://www.gutenberg.org/cache/epub/4583/pg4583.txt"
licence = "public-domain-us"
media_type = "text/plain; charset=utf-8"
sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"

[[document.edition]]
work = "david-hume/dialogues-concerning-natural-religion"
slug = "gutenberg-4583"
title = "Dialogues Concerning Natural Religion"
language = "en"
"#;

fn errors(toml: &str) -> Vec<String> {
    match Manifest::parse(toml) {
        Err(ManifestError::Invalid(errors)) => errors,
        other => panic!("expected validation errors, got {other:?}"),
    }
}

#[test]
fn a_valid_source_list_parses_into_typed_entries() {
    let manifest = Manifest::parse(HUME).unwrap();

    assert_eq!(manifest.thinkers[0].slug, Slug::parse("david-hume").unwrap());
    assert_eq!(manifest.thinkers[0].born_year, Some(1711));
    assert_eq!(manifest.works[0].thinker, Slug::parse("david-hume").unwrap());
    let document = &manifest.documents[0];
    assert_eq!(document.sha256, Some(Sha256::from_hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").unwrap()));
    // Gutenberg documents default to "the book between the markers".
    assert_eq!(document.editions[0].range, RangeSpec::GutenbergBody);
}

#[test]
fn a_document_without_a_pinned_hash_is_allowed_to_parse() {
    // The importer refuses it later and prints the observed hash for review.
    let unpinned = HUME.replace("sha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n", "");

    let manifest = Manifest::parse(&unpinned).unwrap();

    assert_eq!(manifest.documents[0].sha256, None);
}

#[test]
fn every_problem_is_reported_at_once() {
    let broken = HUME
        .replace("slug = \"david-hume\"", "slug = \"David Hume\"") // bad slug
        .replace("\"Q965564\"", "\"Q0\"") // bad Wikidata id
        .replace("ba7816bf", "zz7816bf"); // bad hash

    let errors = errors(&broken);

    assert!(errors.len() >= 3, "{errors:#?}");
    assert!(errors.iter().any(|e| e.contains("David Hume")), "{errors:#?}");
    assert!(errors.iter().any(|e| e.contains("Q0")), "{errors:#?}");
    assert!(errors.iter().any(|e| e.contains("zz7816bf")), "{errors:#?}");
}

#[test]
fn references_must_point_at_something_that_exists() {
    let dangling = HUME.replace(
        "work = \"david-hume/dialogues-concerning-natural-religion\"",
        "work = \"david-hume/treatise-of-human-nature\"",
    );
    assert!(errors(&dangling).iter().any(|e| e.contains("treatise-of-human-nature")));

    let orphan = HUME.replace("thinker = \"david-hume\"", "thinker = \"george-berkeley\"");
    assert!(errors(&orphan).iter().any(|e| e.contains("george-berkeley")));
}

#[test]
fn duplicate_entries_are_refused() {
    let twice = format!("{HUME}\n[[thinker]]\nslug = \"david-hume\"\nname = \"Hume\"\n");
    assert!(errors(&twice).iter().any(|e| e.contains("david-hume") && e.contains("twice")));
}

#[test]
fn non_gutenberg_documents_need_an_explicit_range() {
    let other = HUME.replace("provider = \"gutenberg\"", "provider = \"wikisource\"");
    assert!(errors(&other).iter().any(|e| e.contains("range")));

    // Anchor on the edition's own lines (`original_language = "en"` on the work also
    // contains `language = "en"`).
    let explicit = other.replace(
        "title = \"Dialogues Concerning Natural Religion\"\nlanguage = \"en\"\n",
        "title = \"Dialogues Concerning Natural Religion\"\nlanguage = \"en\"\nrange = { start = 10, end = 500 }\n",
    );
    let manifest = Manifest::parse(&explicit).unwrap();
    assert_eq!(manifest.documents[0].editions[0].range, RangeSpec::Bytes { start: 10, end: 500 });
}

#[test]
fn urls_must_be_https() {
    let insecure = HUME.replace("https://www.gutenberg.org", "http://www.gutenberg.org");
    assert!(errors(&insecure).iter().any(|e| e.contains("https")));
}

#[test]
fn unknown_keys_are_refused_so_typos_cannot_be_ignored() {
    let typo = HUME.replace("wikidata = \"Q37160\"", "wikidta = \"Q37160\"");
    assert!(matches!(Manifest::parse(&typo), Err(ManifestError::Toml(msg)) if msg.contains("wikidta")));
}
