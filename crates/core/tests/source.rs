use sunlit_core::Source;

#[test]
fn citation_names_thinker_work_and_edition() {
    let berkeley = Source {
        thinker: "George Berkeley".into(),
        work: "A Treatise Concerning the Principles of Human Knowledge".into(),
        edition: "Project Gutenberg #4723".into(),
        url: "https://www.gutenberg.org/ebooks/4723".into(),
    };

    assert_eq!(
        berkeley.citation(),
        "George Berkeley, A Treatise Concerning the Principles of Human Knowledge (Project Gutenberg #4723)"
    );
}
