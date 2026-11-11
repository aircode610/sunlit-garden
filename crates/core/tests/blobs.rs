//! Raw-file storage by SHA-256. Most tests use the in-memory backend (fast, no server);
//! `round_trip_through_garage` talks to the real Garage from `.env` (S3_* variables).

use std::sync::Arc;

use bytes::Bytes;
use object_store::memory::InMemory;
use object_store::path::Path;
use object_store::ObjectStoreExt;
use sunlit_core::Sha256;
use sunlit_core::blobs::{BlobError, BlobStore, S3Config};

const BERKELEY: &[u8] = b"It is evident to any one who takes a survey of the objects of human knowledge...";

#[tokio::test]
async fn put_returns_the_content_hash_and_get_returns_the_bytes() {
    let blobs = BlobStore::in_memory();

    let sha = blobs.put(Bytes::from_static(BERKELEY)).await.unwrap();

    assert_eq!(sha, Sha256::of(BERKELEY));
    assert_eq!(blobs.get(&sha).await.unwrap(), BERKELEY);
}

#[tokio::test]
async fn putting_the_same_bytes_twice_is_harmless() {
    let blobs = BlobStore::in_memory();

    let first = blobs.put(Bytes::from_static(BERKELEY)).await.unwrap();
    let second = blobs.put(Bytes::from_static(BERKELEY)).await.unwrap();

    assert_eq!(first, second);
    assert!(blobs.contains(&first).await.unwrap());
}

#[tokio::test]
async fn getting_an_unknown_hash_is_not_found() {
    let blobs = BlobStore::in_memory();
    let missing = Sha256::of(b"never stored");

    let result = blobs.get(&missing).await;

    assert!(matches!(result, Err(BlobError::NotFound(sha)) if sha == missing), "got {result:?}");
    assert!(!blobs.contains(&missing).await.unwrap());
}

#[tokio::test]
async fn bytes_that_no_longer_match_their_key_are_reported_as_corrupt() {
    // Simulate storage corruption or tampering: write other bytes directly under Berkeley's key.
    let backend = Arc::new(InMemory::new());
    let blobs = BlobStore::from_object_store(backend.clone());
    let sha = Sha256::of(BERKELEY);
    backend.put(&Path::from(BlobStore::key(&sha)), b"tampered".to_vec().into()).await.unwrap();

    let result = blobs.get(&sha).await;

    assert!(matches!(result, Err(BlobError::Corrupt { expected, .. }) if expected == sha), "got {result:?}");
}

#[test]
fn keys_are_sha256_slash_hex() {
    let sha = Sha256::of(b"abc");
    assert_eq!(BlobStore::key(&sha), format!("sha256/{}", sha.to_hex()));
}

#[test]
fn blob_store_futures_can_move_between_threads() {
    // A multi-threaded server needs `Send` futures; this fails to compile if they aren't.
    fn assert_send<T: Send>(_: T) {}
    let blobs = BlobStore::in_memory();
    let sha = Sha256::of(b"x");
    assert_send(blobs.get(&sha));
    assert_send(blobs.put(Bytes::new()));
    assert_send(blobs.contains(&sha));
}

#[tokio::test]
async fn round_trip_through_garage() {
    let config = S3Config::from_env().expect("S3_* variables missing: run `set -a; source .env; set +a`");
    let blobs = BlobStore::s3(&config).unwrap();
    // Unique per run so the test never depends on earlier runs.
    let body = format!("round trip {:?}", std::time::SystemTime::now());

    let sha = blobs.put(Bytes::from(body.clone())).await.unwrap();

    assert_eq!(blobs.get(&sha).await.unwrap(), body.as_bytes());
    assert!(blobs.contains(&sha).await.unwrap());
}
