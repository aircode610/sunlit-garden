//! `sunlit score`: how well the plain-text compiler reproduces a hand-structured edition.

use std::path::PathBuf;

use clap::Args;
use sunlit_core::corpus::compile::structure::{Options, PassageUnit, compile_plain_text};
use sunlit_core::corpus::gutenberg;
use sunlit_core::corpus::score::{
    DivisionMark, Report, Segmented, score, segment, spine_order, standard_ebooks_gold,
};

const FIXTURES: &str = "crates/core/tests/fixtures";

#[derive(Args)]
pub struct ScoreArgs {
    /// A Gutenberg plain-text file, compiled with default options (plus --passages).
    #[arg(long, default_value_t = format!("{FIXTURES}/gutenberg/pg9662.txt"))]
    gutenberg: String,
    /// A Standard Ebooks folder: content.opf and the text files of its spine (the answer key).
    #[arg(long, default_value_t = format!("{FIXTURES}/standard-ebooks/hume-enquiry"))]
    standard_ebooks: String,
    /// What one passage is, as the edition would declare it (`auto`, `paragraphs`,
    /// `numbered-sections`). Default `paragraphs`: the Enquiry is cited by paragraph.
    #[arg(long, default_value = "paragraphs")]
    passages: String,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run(args: ScoreArgs) -> Result<()> {
    let file = std::fs::read(&args.gutenberg).map_err(|e| format!("{}: {e}", args.gutenberg))?;
    let passages = match args.passages.as_str() {
        "auto" => PassageUnit::Auto,
        "paragraphs" => PassageUnit::Paragraphs,
        "numbered-sections" => PassageUnit::NumberedSections,
        other => {
            return Err(
                format!("--passages {other:?}: auto, paragraphs or numbered-sections").into(),
            );
        }
    };
    let options = Options {
        passages,
        ..Options::default()
    };
    let structure = compile_plain_text(&file, gutenberg::body_range(&file)?, &options);
    let ours = segment(&file, &structure);

    let dir = PathBuf::from(&args.standard_ebooks);
    let opf = std::fs::read_to_string(dir.join("content.opf"))?;
    let spine: Vec<(String, String)> = spine_order(&opf)?
        .into_iter()
        .filter_map(|id| {
            std::fs::read_to_string(dir.join(&id))
                .ok()
                .map(|text| (id, text))
        })
        .collect();
    let gold = standard_ebooks_gold(&spine)?;

    print(&score(&ours, &gold), &gold);
    Ok(())
}

fn print(r: &Report, gold: &Segmented) {
    let pct = |x: f64| format!("{:.1}%", 100.0 * x);
    let at = |d: &DivisionMark| {
        let w: Vec<&str> = gold
            .words
            .iter()
            .skip(d.at)
            .take(6)
            .map(String::as_str)
            .collect();
        format!(
            "depth {} label {:<6} at word {:>6} “{}…”",
            d.depth,
            d.label,
            d.at,
            w.join(" ")
        )
    };
    println!(
        "aligned words      {} of {} ({})",
        r.aligned_words,
        r.gold_words,
        pct(r.aligned_words as f64 / r.gold_words as f64)
    );
    println!(
        "passages           precision {}  recall {}  F1 {}   ({} found of {}, {} predicted, {} in material the answer key lacks)",
        pct(r.passages.precision),
        pct(r.passages.recall),
        pct(r.passages.f1),
        r.passages.true_positives,
        r.passages.gold,
        r.passages.predicted,
        r.unmapped_passages
    );
    println!(
        "segmentation       Pk {:.4}  WindowDiff {:.4}   (0 = perfect)",
        r.pk, r.window_diff
    );
    println!(
        "divisions          precision {}  recall {}  F1 {}   labels {}  nesting {}",
        pct(r.divisions.precision),
        pct(r.divisions.recall),
        pct(r.divisions.f1),
        pct(r.division_labels),
        pct(r.division_depths)
    );
    for d in &r.missed_divisions {
        println!("  missed   {}", at(d));
    }
    for d in &r.extra_divisions {
        println!("  extra    {}", at(d));
    }
    for (g, o) in &r.wrong_divisions {
        println!(
            "  wrong    {}   ours: depth {} label {}",
            at(g),
            o.depth,
            o.label
        );
    }
}
