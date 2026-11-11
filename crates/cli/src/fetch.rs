//! Real downloads for the importer, over HTTPS with `reqwest`.

use std::time::Duration;

use bytes::Bytes;
use sunlit_core::corpus::import::{FetchError, Fetcher};

/// Refuse anything bigger: a wrong URL must not fill the disk. The largest book we expect is a
/// few MB of plain text.
const MAX_BYTES: u64 = 50 * 1024 * 1024;

pub struct HttpFetcher {
    client: reqwest::Client,
}

impl HttpFetcher {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            // Identify ourselves honestly: providers like Gutenberg ask bots to.
            .user_agent(concat!("sunlit-garden/", env!("CARGO_PKG_VERSION"), " (+https://github.com/aircode610/sunlit-garden)"))
            .timeout(Duration::from_secs(120))
            .https_only(true)
            .build()?;
        Ok(HttpFetcher { client })
    }
}

impl Fetcher for HttpFetcher {
    async fn fetch(&self, url: &str) -> Result<Bytes, FetchError> {
        let err = |e: reqwest::Error| FetchError(e.to_string());
        let response = self.client.get(url).send().await.map_err(err)?.error_for_status().map_err(err)?;
        if response.content_length().is_some_and(|n| n > MAX_BYTES) {
            return Err(FetchError(format!("file is larger than {MAX_BYTES} bytes")));
        }
        let bytes = response.bytes().await.map_err(err)?;
        // Content-Length can be missing or wrong, so check the real size too.
        if bytes.len() as u64 > MAX_BYTES {
            return Err(FetchError(format!("file is larger than {MAX_BYTES} bytes")));
        }
        Ok(bytes)
    }
}
