//! The real source list compiles cleanly: every declaration and expectation in
//! `corpus/sources.toml` is checked against the real files. A mistake in the list fails here.

use sunlit_core::corpus::compile::compile_edition;
use sunlit_core::corpus::manifest::Manifest;

const SOURCES: &str = include_str!("../../../corpus/sources.toml");

/// The pinned file of a document, kept as tests/fixtures/<provider>/<file name in its URL>.
fn fixture(provider: &str, url: &str) -> Vec<u8> {
    let name = url.rsplit('/').next().unwrap_or_default();
    let path = format!(
        "{}/tests/fixtures/{provider}/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|_| panic!("no fixture {path}: add it to tests/fixtures"))
}

#[test]
fn every_edition_in_the_source_list_compiles_without_problems() {
    let manifest = Manifest::parse(SOURCES).unwrap();
    for document in &manifest.documents {
        let file = fixture(document.provider.as_str(), &document.url);
        assert_eq!(
            Some(sunlit_core::Sha256::of(&file)),
            document.sha256,
            "fixture differs from the pin"
        );
        for edition in &document.editions {
            let s = compile_edition(&file, edition).unwrap();
            assert_eq!(
                s.problems,
                Vec::<String>::new(),
                "edition {}",
                edition.slug.as_str()
            );
            assert!(
                edition.compile.expect.is_some(),
                "edition {} should declare expectations",
                edition.slug.as_str()
            );
        }
    }
}
