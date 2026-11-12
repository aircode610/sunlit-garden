//! The compiler's back-end: write a validated structure to the Store.
//!
//! Refuses an edition with problems (it goes to review). Otherwise stores divisions, then
//! passages in planned order — every heading, note or marker after the passage it attaches to,
//! so planned indices can be turned into database ids. Every write is an idempotent `ensure_*`,
//! so storing twice changes nothing, and a different structure is a `Conflict` (re-splitting is
//! deliberate: `Store::clear_edition_structure` first).

use std::fmt;

use super::structure::{PlannedRole, Structure};
use crate::store::{
    DivisionId, Edition, NewDivision, NewPassage, PassageId, PassageRole, Store, StoreError,
};
use crate::values::{InvalidValue, Locator, Sha256};

/// What was stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    pub divisions: usize,
    pub passages: usize,
}

#[derive(Debug)]
pub enum StoreStructureError {
    /// The structure has this many problems; nothing was stored.
    NeedsReview(usize),
    /// A passage's bytes aren't valid UTF-8 (never silently "fixed").
    NotUtf8 {
        locator: String,
    },
    Invalid(InvalidValue),
    Store(StoreError),
}

impl fmt::Display for StoreStructureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreStructureError::NeedsReview(n) => {
                write!(f, "{n} problem(s): edition needs review, nothing stored")
            }
            StoreStructureError::NotUtf8 { locator } => {
                write!(f, "passage {locator} is not valid UTF-8")
            }
            StoreStructureError::Invalid(e) => write!(f, "{e}"),
            StoreStructureError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreStructureError {}

impl From<StoreError> for StoreStructureError {
    fn from(e: StoreError) -> Self {
        StoreStructureError::Store(e)
    }
}

impl From<InvalidValue> for StoreStructureError {
    fn from(e: InvalidValue) -> Self {
        StoreStructureError::Invalid(e)
    }
}

/// Normalisation v1 for `normalized_sha256`: whitespace runs collapsed to one space, trimmed,
/// lowercased (Rust std). Provisional; see the vault decision "Passage normalisation v1".
pub fn normalize_v1(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub async fn store_structure<S: Store>(
    store: &S,
    edition: &Edition,
    file: &[u8],
    structure: &Structure,
) -> Result<Stored, StoreStructureError> {
    if !structure.problems.is_empty() {
        return Err(StoreStructureError::NeedsReview(structure.problems.len()));
    }

    let mut division_ids: Vec<DivisionId> = Vec::with_capacity(structure.compiled.divisions.len());
    for (i, d) in structure.compiled.divisions.iter().enumerate() {
        let new = NewDivision {
            edition: edition.id,
            parent: None,
            kind: d.kind.clone(),
            label: d.label.clone(),
            ordinal: i as i32,
            start_byte: d.range.start as i64,
            end_byte: d.range.end as i64,
        };
        division_ids.push(store.ensure_division(&new).await?.id);
    }

    // Planned index → stored id. Targets are always planned before what attaches to them.
    let mut ids: Vec<PassageId> = Vec::with_capacity(structure.passages.len());
    for (i, p) in structure.passages.iter().enumerate() {
        let target = |of: usize| {
            ids.get(of).copied().ok_or_else(|| InvalidValue {
                kind: "attachment (target planned after its passage)",
                value: p.locator.clone(),
            })
        };
        let role = match p.role {
            PlannedRole::Text => PassageRole::Text,
            PlannedRole::EditorHeading { of } => PassageRole::EditorHeading { of: target(of)? },
            PlannedRole::EditorNote { on } => PassageRole::EditorNote { on: target(on)? },
            PlannedRole::EditorMarker { in_ } => PassageRole::EditorMarker { in_: target(in_)? },
            PlannedRole::AuthorNote { on } => PassageRole::AuthorNote { on: target(on)? },
        };
        let text = std::str::from_utf8(&file[p.range.clone()])
            .map_err(|_| StoreStructureError::NotUtf8 {
                locator: p.locator.clone(),
            })?
            .to_owned();
        // Author text hashes its author words (editor insertions removed); editorial passages
        // hash their own text.
        let for_hash = if p.role == PlannedRole::Text {
            structure.author_words(file, i)
        } else {
            text.clone()
        };
        let new = NewPassage {
            edition: edition.id,
            division: Some(division_ids[p.division]),
            ordinal: i as i32,
            locator: Locator::parse(&p.locator)?,
            role,
            start_byte: p.range.start as i64,
            end_byte: p.range.end as i64,
            text,
            normalized_sha256: Sha256::of(normalize_v1(&for_hash).as_bytes()),
        };
        ids.push(store.ensure_passage(&new).await?.id);
    }

    Ok(Stored {
        divisions: division_ids.len(),
        passages: ids.len(),
    })
}
