//! Sunlit Garden's core: data model, corpus, search, graph and storage.
//! Knows nothing about the UI, so a Rune breakage can never reach this crate.

mod source;
pub mod store;
mod values;

pub use source::Source;
pub use values::{InvalidValue, Sha256, Slug, WikidataId};
