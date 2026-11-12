//! Every annotation file in `corpus/annotations/` stays valid: resolved, inside author words,
//! consistent. A hand edit that breaks the dataset fails here.

use sunlit_core::corpus::compile::compile_edition;
use sunlit_core::corpus::manifest::Manifest;
use sunlit_core::dataset::{Record, check, parse_jsonl};

const SOURCES: &str = include_str!("../../../corpus/sources.toml");
const FILES: [(&str, &str, &[u8]); 2] = [
    ("gutenberg-4723", include_str!("../../../corpus/annotations/gutenberg-4723.jsonl"), include_bytes!("fixtures/gutenberg/pg4723.txt")),
    ("gutenberg-4583", include_str!("../../../corpus/annotations/gutenberg-4583.jsonl"), include_bytes!("fixtures/gutenberg/pg4583.txt")),
];

#[test]
fn every_annotation_file_is_valid() {
    let manifest = Manifest::parse(SOURCES).unwrap();
    for (slug, jsonl, file) in FILES {
        let records = parse_jsonl(jsonl).unwrap_or_else(|e| panic!("{slug}: {e:?}"));
        assert!(matches!(&records[0], Record::Edition(h) if h.slug == slug), "{slug}: header");
        let edition = manifest.documents.iter().flat_map(|d| &d.editions).find(|e| e.slug.as_str() == slug).unwrap();
        let structure = compile_edition(file, edition).unwrap();
        let report = check(&records, file, &structure);
        assert_eq!(report.problems, Vec::<String>::new(), "{slug}");
        assert!(report.units > 0, "{slug}: no units");
    }
}
