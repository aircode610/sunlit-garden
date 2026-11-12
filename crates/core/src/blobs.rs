//! Raw files in object storage, addressed by their SHA-256 (key `sha256/<hex>`).
//!
//! Content addressing gives three guarantees for free:
//! * the same bytes always land under the same key, so uploading twice is harmless;
//! * a key can only ever hold one content, so `get` can prove the bytes are unchanged;
//! * nothing is stored twice.
//!
//! `object_store` hides the provider: Garage locally, Cloudflare R2 or Amazon S3 when hosted,
//! or in memory for tests.

use std::fmt;
use std::sync::Arc;

use bytes::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::memory::InMemory;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt};

use crate::values::Sha256;

/// Where the S3-compatible service is and how to sign requests to it.
#[derive(Clone)]
pub struct S3Config {
    /// e.g. `http://127.0.0.1:3900` (Garage) or `https://<account>.r2.cloudflarestorage.com`.
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl S3Config {
    /// Read `S3_ENDPOINT`, `S3_REGION`, `S3_BUCKET`, `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY`.
    pub fn from_env() -> Result<Self, BlobError> {
        let var = |name: &str| {
            std::env::var(name).map_err(|_| BlobError::Config(format!("{name} is not set")))
        };
        Ok(S3Config {
            endpoint: var("S3_ENDPOINT")?,
            region: var("S3_REGION")?,
            bucket: var("S3_BUCKET")?,
            access_key_id: var("S3_ACCESS_KEY_ID")?,
            secret_access_key: var("S3_SECRET_ACCESS_KEY")?,
        })
    }
}

// Hand-written so the secret never ends up in logs or panic messages.
impl fmt::Debug for S3Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"<redacted>")
            .finish()
    }
}

#[derive(Debug)]
pub enum BlobError {
    /// No object under this hash.
    NotFound(Sha256),
    /// The object's bytes don't hash to its key: storage corruption or tampering.
    Corrupt { expected: Sha256, actual: Sha256 },
    /// Missing or invalid configuration.
    Config(String),
    /// Network, permission or provider failure.
    Storage(object_store::Error),
}

impl fmt::Display for BlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlobError::NotFound(sha) => write!(f, "no blob with sha256 {}", sha.to_hex()),
            BlobError::Corrupt { expected, actual } => {
                write!(
                    f,
                    "blob {} is corrupt: its bytes hash to {}",
                    expected.to_hex(),
                    actual.to_hex()
                )
            }
            BlobError::Config(msg) => write!(f, "object storage config: {msg}"),
            BlobError::Storage(e) => write!(f, "object storage: {e}"),
        }
    }
}

impl std::error::Error for BlobError {}

impl From<object_store::Error> for BlobError {
    fn from(e: object_store::Error) -> Self {
        BlobError::Storage(e)
    }
}

/// Content-addressed blob storage. Cheap to clone: it shares one connection pool.
#[derive(Debug, Clone)]
pub struct BlobStore {
    // `Arc<dyn ObjectStore>`: any backend (S3, memory, …) behind one shared pointer.
    inner: Arc<dyn ObjectStore>,
}

impl BlobStore {
    /// An S3-compatible service (Garage, R2, S3). Plain HTTP is only allowed when the
    /// endpoint itself is `http://`, i.e. local development.
    pub fn s3(config: &S3Config) -> Result<Self, BlobError> {
        let store = AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_allow_http(config.endpoint.starts_with("http://"))
            .with_region(&config.region)
            .with_bucket_name(&config.bucket)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key)
            // Path-style URLs (`endpoint/bucket/key`): what Garage serves locally.
            .with_virtual_hosted_style_request(false)
            .build()
            .map_err(|e| BlobError::Config(e.to_string()))?;
        Ok(BlobStore {
            inner: Arc::new(store),
        })
    }

    /// Everything kept in memory; for tests.
    pub fn in_memory() -> Self {
        BlobStore {
            inner: Arc::new(InMemory::new()),
        }
    }

    /// Wrap any `object_store` backend.
    pub fn from_object_store(store: Arc<dyn ObjectStore>) -> Self {
        BlobStore { inner: store }
    }

    /// The object key for a hash: `sha256/<64 hex chars>`.
    pub fn key(sha: &Sha256) -> String {
        format!("sha256/{}", sha.to_hex())
    }

    fn path(sha: &Sha256) -> Path {
        Path::from(Self::key(sha))
    }

    /// Store `bytes` and return their hash. Skips the upload if the hash is already stored.
    pub async fn put(&self, bytes: Bytes) -> Result<Sha256, BlobError> {
        let sha = Sha256::of(&bytes);
        if !self.contains(&sha).await? {
            self.inner.put(&Self::path(&sha), bytes.into()).await?;
        }
        Ok(sha)
    }

    /// Fetch the bytes for `sha`, verifying they still hash to it.
    pub async fn get(&self, sha: &Sha256) -> Result<Bytes, BlobError> {
        let bytes = match self.inner.get(&Self::path(sha)).await {
            Ok(result) => result.bytes().await?,
            Err(object_store::Error::NotFound { .. }) => return Err(BlobError::NotFound(*sha)),
            Err(e) => return Err(e.into()),
        };
        let actual = Sha256::of(&bytes);
        if actual == *sha {
            Ok(bytes)
        } else {
            Err(BlobError::Corrupt {
                expected: *sha,
                actual,
            })
        }
    }

    /// Whether `sha` is stored (a cheap HEAD request; no download).
    pub async fn contains(&self, sha: &Sha256) -> Result<bool, BlobError> {
        match self.inner.head(&Self::path(sha)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}
