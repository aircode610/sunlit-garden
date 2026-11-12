//! `sunlit annotations …`: the argument-unit (ADU) dataset in `corpus/annotations/`.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use sunlit_core::blobs::{BlobStore, S3Config};
use sunlit_core::corpus::compile::compile_edition;
use sunlit_core::corpus::compile::structure::Structure;
use sunlit_core::corpus::manifest::Manifest;
use sunlit_core::dataset::{self, Record};

#[derive(Subcommand)]
pub enum Action {
    /// Turn every unit's quote into byte offsets and write the file back.
    Resolve {
        file: PathBuf,
        #[arg(long, default_value = "corpus/sources.toml")]
        sources: PathBuf,
    },
    /// Run every mechanical check from the guidelines (exit 1 on problems).
    Check {
        file: PathBuf,
        #[arg(long, default_value = "corpus/sources.toml")]
        sources: PathBuf,
    },
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub async fn run(action: Action) -> Result<()> {
    match action {
        Action::Resolve { file, sources } => {
            let (mut records, bytes, structure) = load(&file, &sources).await?;
            let problems = dataset::resolve(&mut records, &bytes, &structure);
            std::fs::write(&file, dataset::to_jsonl(&records))?;
            let resolved = records
                .iter()
                .filter(|r| matches!(r, Record::Unit(u) if u.start.is_some()))
                .count();
            println!("{}: {resolved} unit(s) resolved", file.display());
            for p in &problems {
                println!("  PROBLEM: {p}");
            }
            if problems.is_empty() {
                Ok(())
            } else {
                Err(format!("{} quote(s) could not be resolved", problems.len()).into())
            }
        }
        Action::Check { file, sources } => {
            let (records, bytes, structure) = load(&file, &sources).await?;
            let report = dataset::check(&records, &bytes, &structure);
            println!(
                "{}: {} / {} author passages with a verdict, {} units, {} relations",
                file.display(),
                report.passages_with_verdict,
                report.author_passages,
                report.units,
                report.relations
            );
            println!("  units by type: {:?}", report.units_by_type);
            for p in &report.problems {
                println!("  PROBLEM: {p}");
            }
            if report.problems.is_empty() {
                Ok(())
            } else {
                Err(format!("{} problem(s)", report.problems.len()).into())
            }
        }
    }
}

/// Load an annotation file and compile the edition its header names, from the pinned file in
/// object storage, so annotations are always checked against the passages the Library stores.
async fn load(file: &Path, sources: &Path) -> Result<(Vec<Record>, bytes::Bytes, Structure)> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let records = dataset::parse_jsonl(&text).map_err(|errors| errors.join("\n"))?;
    let Some(Record::Edition(header)) = records.first() else {
        return Err(format!("{}: first line must be the edition header", file.display()).into());
    };
    let manifest = Manifest::parse(&std::fs::read_to_string(sources)?)?;
    let (document, edition) = manifest
        .documents
        .iter()
        .find_map(|d| {
            d.editions
                .iter()
                .find(|e| e.slug.as_str() == header.slug)
                .map(|e| (d, e))
        })
        .ok_or_else(|| format!("edition {:?} is not in {}", header.slug, sources.display()))?;
    let sha = document.sha256.ok_or("document not pinned")?;
    let bytes = BlobStore::s3(&S3Config::from_env()?)?.get(&sha).await?;
    let structure = compile_edition(&bytes, edition)?;
    if !structure.problems.is_empty() {
        return Err(format!(
            "edition {} has compile problems; fix those first",
            header.slug
        )
        .into());
    }
    Ok((records, bytes, structure))
}
