//! `sunlit`: command-line tools for running Sunlit Garden's data pipeline.
//!
//! ```sh
//! sunlit import                       # imports corpus/sources.toml
//! sunlit import path/to/sources.toml
//! ```
//!
//! Configuration comes from the environment (`DATABASE_URL`, `S3_*`); a `.env` file in the
//! current directory is loaded first if present.

mod fetch;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sunlit_core::blobs::{BlobStore, S3Config};
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
    /// Import the sources listed in a source list into object storage and Postgres.
    /// Safe to re-run: pinned files already stored are not downloaded again.
    Import {
        /// The source list.
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
    }
}

fn how_many(editions: usize) -> String {
    if editions == 1 { "1 edition".into() } else { format!("{editions} editions") }
}
