//! The compiler scorer on the Hume Enquiry test pair: Standard Ebooks is the answer key.

use sunlit_core::corpus::score::{DivisionMark, spine_order, standard_ebooks_gold};

const DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/standard-ebooks/hume-enquiry"
);

fn spine() -> Vec<(String, String)> {
    let opf = std::fs::read_to_string(format!("{DIR}/content.opf")).unwrap();
    // The fixture keeps only the book's text chapters from the spine.
    spine_order(&opf)
        .unwrap()
        .into_iter()
        .filter_map(|id| {
            std::fs::read_to_string(format!("{DIR}/{id}"))
                .ok()
                .map(|t| (id, t))
        })
        .collect()
}

#[test]
fn the_spine_gives_the_reading_order() {
    let opf = std::fs::read_to_string(format!("{DIR}/content.opf")).unwrap();
    let order = spine_order(&opf).unwrap();
    let chapters: Vec<&String> = order
        .iter()
        .filter(|id| id.starts_with("chapter-"))
        .collect();
    assert_eq!(
        chapters,
        (1..=12)
            .map(|n| format!("chapter-{n}.xhtml"))
            .collect::<Vec<_>>()
            .iter()
            .collect::<Vec<_>>()
    );
    assert_eq!(order.first().map(String::as_str), Some("titlepage.xhtml"));
}

#[test]
fn the_answer_key_has_the_books_sections_and_parts() {
    let gold = standard_ebooks_gold(&spine()).unwrap();

    let top: Vec<&str> = gold
        .divisions
        .iter()
        .filter(|d| d.depth == 1)
        .map(|d| d.label.as_str())
        .collect();
    assert_eq!(
        top,
        (1..=12)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    // Parts: Sections IV, V, VII, VIII, X have two, XII has three.
    let parts = |section: &str| {
        let i = gold
            .divisions
            .iter()
            .position(|d| d.depth == 1 && d.label == section)
            .unwrap();
        gold.divisions[i + 1..]
            .iter()
            .take_while(|d| d.depth == 2)
            .count()
    };
    let counts: Vec<usize> = (1..=12).map(|n| parts(&n.to_string())).collect();
    assert_eq!(counts, [0, 0, 0, 2, 2, 0, 2, 2, 0, 2, 0, 3]);

    // Section X, Part I opens "There is, in Dr. Tillotson's writings, an argument…".
    let x1 = gold
        .divisions
        .iter()
        .position(|d| d.depth == 1 && d.label == "10")
        .unwrap()
        + 1;
    let DivisionMark { at, .. } = gold.divisions[x1];
    assert_eq!(
        gold.words[at..at + 6],
        ["there", "is", "in", "dr", "tillotsons", "writings"]
    );
    assert!(
        gold.boundaries.contains(&at),
        "a passage starts where the Part starts"
    );
}

const GUTENBERG_9662: &[u8] = include_bytes!("fixtures/gutenberg/pg9662.txt");

fn compiled_9662() -> sunlit_core::corpus::compile::structure::Structure {
    use sunlit_core::corpus::compile::structure::{Options, PassageUnit, compile_plain_text};
    let body = sunlit_core::corpus::gutenberg::body_range(GUTENBERG_9662).unwrap();
    // As an import would declare it: the Enquiry is cited by paragraph (EHU 10.1).
    let options = Options {
        passages: PassageUnit::Paragraphs,
        ..Options::default()
    };
    compile_plain_text(GUTENBERG_9662, body, &options)
}

#[test]
fn a_compiled_edition_becomes_the_same_shape() {
    use sunlit_core::corpus::compile::structure::PlannedRole;
    use sunlit_core::corpus::score::{segment, words};
    let structure = compiled_9662();

    let seg = segment(GUTENBERG_9662, &structure);

    let texts: Vec<usize> = (0..structure.passages.len())
        .filter(|&i| structure.passages[i].role == PlannedRole::Text)
        .collect();
    let expected: Vec<String> = texts
        .iter()
        .flat_map(|&i| words(&structure.author_words(GUTENBERG_9662, i)).collect::<Vec<_>>())
        .collect();
    assert_eq!(seg.words, expected);
    let non_empty = texts
        .iter()
        .filter(|&&i| {
            words(&structure.author_words(GUTENBERG_9662, i))
                .next()
                .is_some()
        })
        .count();
    assert_eq!(seg.boundaries.len(), non_empty);
    assert!(
        seg.boundaries.windows(2).all(|w| w[0] < w[1]),
        "boundaries ascend"
    );
    assert_eq!(seg.divisions.len(), structure.compiled.divisions.len());
    // Parts nest inside their Section.
    assert!(seg.divisions.iter().any(|d| d.depth == 2));
    assert_eq!(seg.divisions[0].label, "1");
}

fn w(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn alignment_pairs_the_same_words_in_order() {
    use sunlit_core::corpus::score::align;
    // Spelling differs ("shew"/"show"), b has an extra note, a has an extra word.
    let a = w("it is easy to shew that we have been a great deal too liberal");
    let b = w("it is easy to show that 1 we have been a great deal too liberal in our concession");

    let pairs = align(&a, &b);

    assert!(
        pairs.windows(2).all(|p| p[0].0 < p[1].0 && p[0].1 < p[1].1),
        "monotone"
    );
    assert!(
        pairs.iter().all(|&(i, j)| a[i] == b[j]),
        "only equal words are paired"
    );
    assert_eq!(
        pairs.len(),
        13,
        "every shared word except the spelling variant"
    );
}

#[test]
fn the_two_editions_align_almost_completely() {
    use sunlit_core::corpus::score::{align, segment};
    let ours = segment(GUTENBERG_9662, &compiled_9662());
    let gold = standard_ebooks_gold(&spine()).unwrap();

    let pairs = align(&ours.words, &gold.words);

    assert!(
        pairs.windows(2).all(|p| p[0].0 < p[1].0 && p[0].1 < p[1].1),
        "monotone"
    );
    assert!(
        pairs.iter().all(|&(i, j)| ours.words[i] == gold.words[j]),
        "only equal words are paired"
    );
    let covered = pairs.len() as f64 / gold.words.len() as f64;
    println!(
        "aligned {} of {} answer-key words ({covered:.4}); ours has {}",
        pairs.len(),
        gold.words.len(),
        ours.words.len()
    );
    assert!(
        covered > 0.9,
        "{covered:.3} of the answer key's words aligned"
    );
}

#[test]
fn the_answer_key_scores_perfectly_against_itself() {
    use sunlit_core::corpus::score::score;
    let gold = standard_ebooks_gold(&spine()).unwrap();

    let r = score(&gold, &gold);

    assert_eq!(
        (r.passages.precision, r.passages.recall, r.passages.f1),
        (1.0, 1.0, 1.0)
    );
    assert_eq!((r.pk, r.window_diff), (0.0, 0.0));
    assert_eq!(
        (r.divisions.f1, r.division_labels, r.division_depths),
        (1.0, 1.0, 1.0)
    );
}

#[test]
fn a_misplaced_boundary_and_a_flat_part_are_counted() {
    use sunlit_core::corpus::score::{DivisionMark, Segmented, score};
    let text: Vec<String> = (0..20).map(|n| format!("w{n}")).collect();
    let mark = |at, depth, label: &str| DivisionMark {
        at,
        depth,
        label: label.into(),
    };
    let gold = Segmented {
        words: text.clone(),
        boundaries: vec![0, 5, 10, 15],
        divisions: vec![mark(0, 1, "1"), mark(10, 1, "2"), mark(10, 2, "1")],
    };
    // One passage break misplaced (10 -> 12); the Part is not nested.
    let ours = Segmented {
        words: text,
        boundaries: vec![0, 5, 12, 15],
        divisions: vec![mark(0, 1, "1"), mark(10, 1, "2"), mark(10, 1, "1")],
    };

    let r = score(&ours, &gold);

    assert_eq!(
        (
            r.passages.true_positives,
            r.passages.predicted,
            r.passages.gold
        ),
        (3, 4, 4)
    );
    assert_eq!(r.passages.f1, 0.75);
    assert!(r.pk > 0.0 && r.window_diff > 0.0);
    assert_eq!(
        r.divisions.f1, 1.0,
        "all three divisions start in the right place"
    );
    assert_eq!(r.division_labels, 1.0);
    assert!(
        (r.division_depths - 2.0 / 3.0).abs() < 1e-9,
        "the Part is at the wrong depth"
    );
}

#[test]
fn a_word_split_differently_at_a_boundary_still_maps_there() {
    use sunlit_core::corpus::score::{DivisionMark, Segmented, score};
    // Gutenberg prints "every one", Standard Ebooks "everyone", right where Section II starts.
    let gold = Segmented {
        words: w("the end of one section everyone will readily allow that"),
        boundaries: vec![0, 5],
        divisions: vec![DivisionMark {
            at: 5,
            depth: 1,
            label: "2".into(),
        }],
    };
    let ours = Segmented {
        words: w("the end of one section every one will readily allow that"),
        boundaries: vec![0, 5],
        divisions: vec![DivisionMark {
            at: 5,
            depth: 1,
            label: "2".into(),
        }],
    };

    let r = score(&ours, &gold);

    assert_eq!(r.passages.f1, 1.0, "{r:?}");
    assert_eq!(r.divisions.f1, 1.0, "{r:?}");
}

/// The plain-text compiler's score on the Enquiry (default options) may only go up. When a
/// compiler change improves it, raise these floors in the same commit.
#[test]
fn the_compiler_does_not_get_worse_on_the_enquiry() {
    use sunlit_core::corpus::score::{score, segment};
    let r = score(
        &segment(GUTENBERG_9662, &compiled_9662()),
        &standard_ebooks_gold(&spine()).unwrap(),
    );

    let floors = [
        ("passage precision", r.passages.precision, 1.0),
        ("passage recall", r.passages.recall, 1.0),
        ("division F1", r.divisions.f1, 1.0),
        ("division labels", r.division_labels, 1.0),
        ("division nesting", r.division_depths, 1.0),
    ];
    for (name, value, floor) in floors {
        assert!(
            value >= floor - 1e-9,
            "{name} dropped to {value:.4} (floor {floor})"
        );
    }
    assert!(
        r.window_diff <= 0.0 + 1e-9,
        "WindowDiff rose to {:.4}",
        r.window_diff
    );
}

// ---- The held-out pair: Hume's Treatise. Score it; never tune the compiler on it. -------------

const TREATISE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/standard-ebooks/hume-treatise"
);

fn treatise_spine() -> Vec<(String, String)> {
    let opf = std::fs::read_to_string(format!("{TREATISE}/content.opf")).unwrap();
    spine_order(&opf)
        .unwrap()
        .into_iter()
        .filter_map(|id| {
            std::fs::read_to_string(format!("{TREATISE}/{id}"))
                .ok()
                .map(|t| (id, t))
        })
        .collect()
}

#[test]
fn nesting_across_files_and_front_matter_are_read_from_the_answer_key() {
    let gold = standard_ebooks_gold(&treatise_spine()).unwrap();
    let d = &gold.divisions;

    let top: Vec<&str> = d
        .iter()
        .filter(|x| x.depth == 1)
        .map(|x| x.label.as_str())
        .collect();
    assert_eq!(
        top,
        ["advertisement", "introduction", "1", "2", "3", "appendix"]
    );
    // Book I holds Parts I-IV; Book I Part I holds Sections I-VII (nesting via data-parent).
    let book1 = d
        .iter()
        .position(|x| x.depth == 1 && x.label == "1")
        .unwrap();
    let parts = d[book1 + 1..]
        .iter()
        .take_while(|x| x.depth > 1)
        .filter(|x| x.depth == 2)
        .count();
    assert_eq!(parts, 4);
    let sections = d[book1 + 2..].iter().take_while(|x| x.depth > 2).count();
    assert_eq!(sections, 7);
}

/// Compiles a Gutenberg fixture as an import would declare it (cited by paragraph) and scores it
/// against a Standard Ebooks fixture folder.
fn score_by_paragraph(file: &[u8], standard_ebooks: &str) -> sunlit_core::corpus::score::Report {
    use sunlit_core::corpus::compile::structure::{Options, PassageUnit, compile_plain_text};
    use sunlit_core::corpus::score::{score, segment};
    let options = Options {
        passages: PassageUnit::Paragraphs,
        ..Options::default()
    };
    let s = compile_plain_text(
        file,
        sunlit_core::corpus::gutenberg::body_range(file).unwrap(),
        &options,
    );
    let dir = format!(
        "{}/tests/fixtures/standard-ebooks/{standard_ebooks}",
        env!("CARGO_MANIFEST_DIR")
    );
    let opf = std::fs::read_to_string(format!("{dir}/content.opf")).unwrap();
    let spine: Vec<(String, String)> = spine_order(&opf)
        .unwrap()
        .into_iter()
        .filter_map(|id| {
            std::fs::read_to_string(format!("{dir}/{id}"))
                .ok()
                .map(|t| (id, t))
        })
        .collect();
    score(&segment(file, &s), &standard_ebooks_gold(&spine).unwrap())
}

/// Every measure must stay at or above its floor (WindowDiff: at or below).
fn assert_floors(r: &sunlit_core::corpus::score::Report, floors: [f64; 5], window_diff: f64) {
    let values = [
        ("passage precision", r.passages.precision),
        ("passage recall", r.passages.recall),
        ("division F1", r.divisions.f1),
        ("division labels", r.division_labels),
        ("division nesting", r.division_depths),
    ];
    for ((name, value), floor) in values.into_iter().zip(floors) {
        assert!(
            value >= floor - 1e-9,
            "{name} dropped to {value:.4} (floor {floor})"
        );
    }
    assert!(
        r.window_diff <= window_diff + 1e-9,
        "WindowDiff rose to {:.4}",
        r.window_diff
    );
}

/// The held-out score: a regression floor only. **Never change the compiler to raise it** — fix
/// failures on a tuning book with the same pattern, then see whether this number moves.
/// Precision 0.982 → 0.980 with the Moral Sentiments heading fixes: the Advertisement,
/// Introduction and Book I's opening are no longer swallowed by the contents list (+23 correct
/// boundaries), but three of their headings ("BOOK I. OF THE UNDERSTANDING") carry the title on
/// the heading line, which the compiler doesn't know yet, and become passages.
#[test]
fn the_held_out_treatise_does_not_get_worse() {
    let r = score_by_paragraph(
        include_bytes!("fixtures/gutenberg/pg4705.txt"),
        "hume-treatise",
    );
    assert_floors(&r, [0.980, 0.994, 0.956, 0.028, 0.037], 0.0056);
}

/// A tuning book: heading fixes are made on it. The 1777 Gutenberg text differs from Standard
/// Ebooks' 1790 one (Part III revised, Part VI ↔ VII), so it can't reach 100%.
#[test]
fn moral_sentiments_does_not_get_worse() {
    let r = score_by_paragraph(
        include_bytes!("fixtures/gutenberg/pg67363.txt"),
        "smith-moral-sentiments",
    );
    assert_floors(&r, [0.835, 0.789, 0.847, 0.718, 0.281], 0.1080);
}
