//! Small validated value types. Each one can only be built through `parse`, so a value that
//! exists in Rust is already valid — the same rule the database's CHECK constraints enforce.

use std::fmt;

use sha2::Digest;

/// Why a string was rejected: which kind of value, and the offending input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidValue {
    pub kind: &'static str,
    pub value: String,
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {:?}", self.kind, self.value)
    }
}

impl std::error::Error for InvalidValue {}

/// A URL-safe name: lowercase ASCII letters and digits, in groups joined by single hyphens.
/// Mirrors the SQL check `slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Slug(String);

impl Slug {
    pub fn parse(s: &str) -> Result<Self, InvalidValue> {
        // Splitting on '-' gives the groups; every group must be non-empty and made of
        // [a-z0-9]. An empty group means a leading, trailing or doubled hyphen (or "").
        let valid = s
            .split('-')
            .all(|group| !group.is_empty() && group.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
        if valid {
            Ok(Slug(s.to_owned()))
        } else {
            Err(InvalidValue { kind: "slug", value: s.to_owned() })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A Wikidata item id ("Q37160" = David Hume): the identity anchor that stops the same
/// thinker or work being created twice under different spellings.
/// Mirrors the SQL check `wikidata_id ~ '^Q[1-9][0-9]*$'`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WikidataId(String);

impl WikidataId {
    pub fn parse(s: &str) -> Result<Self, InvalidValue> {
        // `strip_prefix` returns the rest after "Q", or None if it doesn't start with "Q".
        let valid = match s.strip_prefix('Q') {
            Some(digits) => {
                !digits.is_empty() && !digits.starts_with('0') && digits.bytes().all(|b| b.is_ascii_digit())
            }
            None => false,
        };
        if valid {
            Ok(WikidataId(s.to_owned()))
        } else {
            Err(InvalidValue { kind: "Wikidata id", value: s.to_owned() })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A SHA-256 digest: how raw files are named in object storage and how we prove a passage's
/// text is unchanged. Stored in Postgres as 32-byte `bytea`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    /// Hash some bytes.
    pub fn of(bytes: &[u8]) -> Self {
        Sha256(sha2::Sha256::digest(bytes).into())
    }

    /// Rebuild a digest read back from the database; anything but 32 bytes is corrupt data.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, InvalidValue> {
        // `try_into` converts a slice into a fixed-size array only if the lengths match.
        bytes.try_into().map(Sha256).map_err(|_| InvalidValue {
            kind: "SHA-256 (need 32 bytes)",
            value: format!("{} bytes", bytes.len()),
        })
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, the usual way to write a digest (and our object-storage key).
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

// Debug as hex, not as a list of 32 numbers.
impl fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256({})", self.to_hex())
    }
}
