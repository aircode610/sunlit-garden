//! The scorer's test pair: Hume's *Enquiry* as Gutenberg #9662 (plain text, via the official
//! mirror) and as Standard Ebooks (hand-structured XHTML, the answer key) at commit
//! a48776f406d323f044aa8fe3c6029fa36b30347d of
//! github.com/standardebooks/david-hume_an-enquiry-concerning-human-understanding.
//! Pinned by SHA-256 so a fixture can't change silently.

use sunlit_core::Sha256;

const PINNED: [(&str, &str); 14] = [
    (
        "gutenberg/pg9662.txt",
        "647459bc720d0fe09f85ac17b8d7edc2557ff89ba4467151407a4d48228be2cb",
    ),
    (
        "standard-ebooks/hume-enquiry/content.opf",
        "64004ee81ee99ba663b281bd5e1b62db61ba36eeb16fc224d3a28868cacaf20a",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-1.xhtml",
        "5bf5d8a4cd31173892a36998a57fe38a1e134f2f5f946de66a4f66aebe490a44",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-2.xhtml",
        "db48c721e6369323bf9008d0815eec9c69aaaedc3e8976a6c90d9c1e030eb61b",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-3.xhtml",
        "88f3f465801fc18deb9d0b99bd1e39c2733507ef7685c4d8f99457f0c2bafea3",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-4.xhtml",
        "7968d4a7d0cb33c84fa26c2946fa0adc06cf5a54a63eb4a00e8b92e6d6119ee4",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-5.xhtml",
        "11623dafd3d1c2b7c786043de2e5d48c5e937ed89f9a79e38d5f394741d0fd07",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-6.xhtml",
        "33e073ee6e5d46a3529d43dc4331ac3481fb11c53f2ea7ad264236da9301b55a",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-7.xhtml",
        "2a56c500ce6150a7b7b05bd88bfbdd325132ae9fd3a8f8679a657dc7764109cc",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-8.xhtml",
        "415602ba6b75e4d5f9926460f6bb22df397b1ddbd2476f94f54ba77ef3ba3cda",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-9.xhtml",
        "9f5f0eec5234767e39786209b69f95ad4b7eefa6a6140b97646a6c834a1fafb8",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-10.xhtml",
        "be2bd75c7cfb3874433291e983c17478b2302c5c4b27b3a27c65dc42dcc8a4cb",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-11.xhtml",
        "0ab2c413e1ea94101b3b924fc886b90e9d02a8e9a853549e82ae574e50bf1047",
    ),
    (
        "standard-ebooks/hume-enquiry/chapter-12.xhtml",
        "de5bbe3f78fa4934f5761aa099a1fd92879f0cf38d6cc6d9ba5bf025c2792567",
    ),
];

#[test]
fn fixtures_are_the_pinned_files() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (file, sha) in PINNED {
        let bytes = std::fs::read(dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(Sha256::of(&bytes).to_hex(), sha, "{file} changed");
    }
}
