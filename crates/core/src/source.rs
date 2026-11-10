/// Where a text comes from: every passage in the library traces back to one of these.
pub struct Source {
    pub thinker: String,
    pub work: String,
    /// The exact edition we ingested, e.g. "Project Gutenberg #4723".
    pub edition: String,
    pub url: String,
}

impl Source {
    /// A one-line human-readable citation.
    pub fn citation(&self) -> String {
        format!("{}, {} ({})", self.thinker, self.work, self.edition)
    }
}
