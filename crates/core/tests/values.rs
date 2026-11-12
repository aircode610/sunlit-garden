use sunlit_core::{Sha256, Slug, WikidataId};

#[test]
fn sha256_matches_the_standard_test_vector() {
    // FIPS 180-2, Appendix B.1: SHA-256("abc").
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(Sha256::of(b"abc").to_hex(), expected);
}

#[test]
fn sha256_from_database_bytes_must_be_exactly_32_bytes() {
    let digest = Sha256::of(b"abc");
    assert_eq!(Sha256::from_slice(digest.as_bytes()).unwrap(), digest);
    assert!(Sha256::from_slice(&[0u8; 31]).is_err());
}

#[test]
fn wikidata_id_is_q_followed_by_a_number_without_leading_zero() {
    assert_eq!(WikidataId::parse("Q37160").unwrap().as_str(), "Q37160"); // David Hume
    for bad in ["", "Q", "Q0", "Q012", "q37160", "37160", "Q37160x", "P31"] {
        assert!(
            WikidataId::parse(bad).is_err(),
            "{bad:?} should be rejected"
        );
    }
}

#[test]
fn slug_accepts_lowercase_words_joined_by_single_hyphens() {
    for ok in ["david-hume", "gutenberg-4583", "berkeley", "part-10"] {
        assert_eq!(Slug::parse(ok).unwrap().as_str(), ok);
    }
}

#[test]
fn slug_rejects_anything_the_database_would_reject() {
    for bad in [
        "",
        "David-Hume",
        "david hume",
        "david--hume",
        "-hume",
        "hume-",
        "d'holbach",
        "gödel",
    ] {
        assert!(Slug::parse(bad).is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn sha256_parses_from_hex_as_written_in_the_source_list() {
    let hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(Sha256::from_hex(hex).unwrap(), Sha256::of(b"abc"));
    assert_eq!(
        Sha256::from_hex(&hex.to_uppercase()).unwrap(),
        Sha256::of(b"abc"),
        "case-insensitive"
    );
    for bad in [
        "",
        "ba78",
        &hex[1..],
        &format!("{hex}00"),
        &hex.replace('b', "g"),
        &format!("+a{}", &hex[2..]),
    ] {
        assert!(Sha256::from_hex(bad).is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn locator_is_slugs_joined_by_slashes() {
    use sunlit_core::Locator;
    for ok in ["part-10/3", "introduction/3", "part-1/3/note-1", "preface"] {
        assert_eq!(Locator::parse(ok).unwrap().as_str(), ok);
    }
    for bad in [
        "",
        "/",
        "part-1/",
        "/3",
        "part-1//3",
        "part 1/3",
        "Part-1/3",
        "part-1/3/",
    ] {
        assert!(Locator::parse(bad).is_err(), "{bad:?} should be rejected");
    }
}
