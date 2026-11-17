//! `sunlit discover …`: suggest Gutenberg books from its catalogue. Prints candidates only —
//! choosing, pinning and importing stay separate, human-approved steps.

use std::collections::HashSet;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;

use clap::Args;
use flate2::read::GzDecoder;
use sunlit_core::Sha256;
use sunlit_core::corpus::catalog::{self, Query};
use sunlit_core::corpus::import::Fetcher;
use sunlit_core::corpus::manifest::Manifest;

use crate::fetch::HttpFetcher;

/// Canonical address; downloaded through the source list's Gutenberg mirror.
const CATALOG_URL: &str = "https://www.gutenberg.org/cache/epub/feeds/pg_catalog.csv.gz";

/// The catalogue is ~21 MB unzipped; refuse anything absurd (a corrupt or hostile gzip).
const MAX_UNZIPPED: u64 = 200 * 1024 * 1024;

#[derive(Args)]
pub struct DiscoverArgs {
    /// Language codes, any of (`--language en`).
    #[arg(long, value_delimiter = ',')]
    language: Vec<String>,
    /// Library of Congress class prefixes, any of (`--locc B` also matches BL, BX).
    #[arg(long, value_delimiter = ',')]
    locc: Vec<String>,
    /// Class prefixes that don't count (a book is dropped only if all its classes are excluded).
    #[arg(long, value_delimiter = ',')]
    exclude_locc: Vec<String>,
    /// Text in the authors field, ignoring case (`--author "hume, david"`).
    #[arg(long)]
    author: Option<String>,
    /// Read a local catalogue (`.csv` or `.csv.gz`) instead of downloading it.
    #[arg(long)]
    catalog: Option<PathBuf>,
    /// The source list: its mirror is used, and books already in it are marked `*`.
    #[arg(long, default_value = "corpus/sources.toml")]
    sources: PathBuf,
    /// Print at most this many books (the total is always shown).
    #[arg(long)]
    limit: Option<usize>,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub async fn run(args: DiscoverArgs) -> Result<()> {
    let manifest = Manifest::parse(
        &std::fs::read_to_string(&args.sources)
            .map_err(|e| format!("{}: {e}", args.sources.display()))?,
    )?;
    let (raw, from) = match &args.catalog {
        Some(path) => (std::fs::read(path)?, path.display().to_string()),
        None => {
            let url = manifest.mirrored("gutenberg", CATALOG_URL);
            let bytes = HttpFetcher::new()?
                .fetch(&url)
                .await
                .map_err(|e| format!("download failed for {url}: {}", e.0))?;
            (bytes.to_vec(), url)
        }
    };
    let csv = unzip_if_gzip(&raw)?;
    let entries = catalog::parse(&csv)?;

    let query = Query {
        languages: args.language,
        locc: args.locc,
        exclude_locc: args.exclude_locc,
        author: args.author,
    };
    let in_sources: HashSet<&str> = manifest
        .documents
        .iter()
        .filter(|d| d.provider.as_str() == "gutenberg")
        .map(|d| d.provider_id.as_str())
        .collect();
    let mut found: Vec<_> = entries.iter().filter(|e| query.matches(e)).collect();
    found.sort_by_key(|e| e.id);

    // Output is often piped (`| head`, `| grep`): a reader that stops early is not an error.
    let listing = Listing {
        limit: args.limit,
        sources: &args.sources,
        from: &from,
        total: entries.len(),
        raw: &raw,
    };
    match print(&found, &in_sources, &listing) {
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

/// What the summary line reports.
struct Listing<'a> {
    limit: Option<usize>,
    sources: &'a std::path::Path,
    from: &'a str,
    total: usize,
    raw: &'a [u8],
}

fn print(
    found: &[&catalog::CatalogEntry],
    in_sources: &HashSet<&str>,
    listing: &Listing,
) -> std::io::Result<()> {
    let Listing {
        limit,
        sources,
        from,
        total,
        raw,
    } = listing;
    let mut out = std::io::stdout().lock();

    for e in found.iter().take(limit.unwrap_or(usize::MAX)) {
        let mark = if in_sources.contains(e.id.to_string().as_str()) {
            '*'
        } else {
            ' '
        };
        let title = e.title.lines().next().unwrap_or_default();
        let author = e.authors.first().map_or("", String::as_str);
        writeln!(
            out,
            "{:>6} {mark} {:<60.60}  {:<32.32}  {}",
            e.id,
            title,
            author,
            e.locc.join("; ")
        )?;
    }
    let marked = found
        .iter()
        .filter(|e| in_sources.contains(e.id.to_string().as_str()))
        .count();
    writeln!(
        out,
        "{} book(s), {marked} already in {} (*) · catalogue {from} ({total} entries, sha256 {})",
        found.len(),
        sources.display(),
        Sha256::of(raw).to_hex()
    )
}

/// Gzip files start with the bytes 1f 8b; anything else is taken as plain CSV.
fn unzip_if_gzip(raw: &[u8]) -> Result<Vec<u8>> {
    if !raw.starts_with(&[0x1f, 0x8b]) {
        return Ok(raw.to_vec());
    }
    let mut out = Vec::new();
    GzDecoder::new(raw)
        .take(MAX_UNZIPPED + 1)
        .read_to_end(&mut out)?;
    if out.len() as u64 > MAX_UNZIPPED {
        return Err(format!("catalogue unzips to more than {MAX_UNZIPPED} bytes").into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;

    use super::*;

    const CSV: &[u8] = b"Text#,Type\n1,Text\n";

    #[test]
    fn plain_csv_passes_through_and_gzip_is_unzipped() {
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(CSV).unwrap();
        let gz = gz.finish().unwrap();

        assert_eq!(unzip_if_gzip(CSV).unwrap(), CSV);
        assert_eq!(unzip_if_gzip(&gz).unwrap(), CSV);
    }
}
