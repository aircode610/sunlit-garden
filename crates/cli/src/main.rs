//! `sunlit`: command-line tools for running Sunlit Garden's data pipeline.
//!
//! ```sh
//! sunlit import                       # imports corpus/sources.toml
//! sunlit import path/to/sources.toml
//! sunlit compile                      # structure report for every edition (stores nothing)
//! ```
//!
//! Configuration comes from the environment (`DATABASE_URL`, `S3_*`); a `.env` file in the
//! current directory is loaded first if present.

mod annotations;
mod fetch;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sunlit_core::blobs::{BlobStore, S3Config};
use sunlit_core::corpus::compile::compile_edition;
use sunlit_core::corpus::compile::structure::PlannedRole;
use sunlit_core::corpus::import::import;
use sunlit_core::corpus::manifest::Manifest;
use sunlit_core::store::PgStore;

#[derive(Parser)]
#[command(name = "sunlit", about = "Sunlit Garden data tools", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The argument-unit (ADU) dataset in corpus/annotations/.
    Annotations {
        #[command(subcommand)]
        action: annotations::Action,
    },
    /// Import the sources listed in a source list into object storage and Postgres.
    /// Safe to re-run: pinned files already stored are not downloaded again.
    Import {
        /// The source list.
        #[arg(default_value = "corpus/sources.toml")]
        sources: PathBuf,
    },
    /// Compile every edition's structure from its stored file and print a report: divisions,
    /// passages, problems and warnings. Stores nothing. Exits with an error if any edition has
    /// problems (it would go to review).
    Compile {
        #[arg(default_value = "corpus/sources.toml")]
        sources: PathBuf,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    // `.ok()`: a missing .env is fine (e.g. in production the environment is set directly).
    dotenvy::dotenv().ok();
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

// `Box<dyn Error>`: any error type, so `?` works on every step; main prints it.
async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Annotations { action } => annotations::run(action).await,
        Command::Import { sources } => {
            let text = std::fs::read_to_string(&sources).map_err(|e| format!("{}: {e}", sources.display()))?;
            let manifest = Manifest::parse(&text)?;

            let database_url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is not set")?;
            let store = PgStore::connect(&database_url).await?;
            store.migrate().await?;
            let blobs = BlobStore::s3(&S3Config::from_env()?)?;
            let fetcher = fetch::HttpFetcher::new()?;

            let report = import(&manifest, &store, &blobs, &fetcher).await?;

            for doc in &report.documents {
                let how = if doc.downloaded { "downloaded" } else { "already stored" };
                println!("{} {} ({how}, sha256 {})", doc.provider_id, how_many(doc.editions.len()), doc.sha256.to_hex());
                for (slug, range) in &doc.editions {
                    println!("    edition {}: bytes {}..{}", slug.as_str(), range.start, range.end);
                }
            }
            println!(
                "imported {} thinker(s), {} work(s), {} document(s)",
                manifest.thinkers.len(),
                manifest.works.len(),
                report.documents.len()
            );
            Ok(())
        }
        Command::Compile { sources } => {
            let text = std::fs::read_to_string(&sources).map_err(|e| format!("{}: {e}", sources.display()))?;
            let manifest = Manifest::parse(&text)?;
            let blobs = BlobStore::s3(&S3Config::from_env()?)?;
            let needing_review = compile_report(&manifest, &blobs).await?;
            if needing_review > 0 {
                return Err(format!("{needing_review} edition(s) need review").into());
            }
            Ok(())
        }
    }
}

/// `sunlit compile`: the structure report. Returns how many editions have problems.
async fn compile_report(manifest: &Manifest, blobs: &BlobStore) -> Result<usize, Box<dyn std::error::Error>> {
    let mut needing_review = 0;
    for document in &manifest.documents {
        let sha = document.sha256.ok_or_else(|| format!("{} is not pinned; run `sunlit import` first", document.url))?;
        // `get` re-verifies the hash, so we compile exactly the pinned bytes.
        let file = blobs.get(&sha).await?;
        for edition in &document.editions {
            let s = compile_edition(&file, edition)?;
            let verdict = if s.problems.is_empty() { "OK" } else { "NEEDS REVIEW" };
            println!("{} ({} #{}): {verdict}", edition.slug.as_str(), document.provider.as_str(), document.provider_id);
            let role_count = |f: fn(&PlannedRole) -> bool| s.passages.iter().filter(|p| f(&p.role)).count();
            println!(
                "  passages: {} text, {} editor headings, {} editor notes, {} editor markers, {} author notes",
                role_count(|r| matches!(r, PlannedRole::Text)),
                role_count(|r| matches!(r, PlannedRole::EditorHeading { .. })),
                role_count(|r| matches!(r, PlannedRole::EditorNote { .. })),
                role_count(|r| matches!(r, PlannedRole::EditorMarker { .. })),
                role_count(|r| matches!(r, PlannedRole::AuthorNote { .. })),
            );
            let divisions: Vec<String> = s
                .compiled
                .divisions
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let n = s.passages.iter().filter(|p| p.division == i && p.role == PlannedRole::Text).count();
                    format!("{} ({n})", d.slug)
                })
                .collect();
            println!("  divisions: {}", divisions.join(", "));
            if !s.toc.is_empty() {
                println!("  contents list cross-checked: {} entries", s.toc.len());
            }
            for p in &s.problems {
                println!("  PROBLEM: {p}");
            }
            for w in &s.warnings {
                println!("  warning: {w}");
            }
            if !s.problems.is_empty() {
                needing_review += 1;
            }
        }
    }
    Ok(needing_review)
}

fn how_many(editions: usize) -> String {
    if editions == 1 { "1 edition".into() } else { format!("{editions} editions") }
}
