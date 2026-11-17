//! Real downloads for the importer, over HTTPS with `reqwest`.

use std::time::Duration;

use bytes::Bytes;
use sunlit_core::corpus::import::{FetchError, Fetcher};
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Refuse anything bigger: a wrong URL must not fill the disk. The largest book we expect is a
/// few MB of plain text.
const MAX_BYTES: u64 = 50 * 1024 * 1024;

/// Minimum time between two requests: polite to providers (Gutenberg's mirrors are run by
/// volunteers) and well below any rate limit. Imports are rare, so speed doesn't matter here.
const GAP: Duration = Duration::from_secs(2);

pub struct HttpFetcher {
    client: reqwest::Client,
    pacer: Pacer,
}

/// Spaces calls to [`Pacer::wait`] at least `gap` apart; the first returns at once.
struct Pacer {
    gap: Duration,
    /// When the previous call was let through. The lock is held while sleeping, so concurrent
    /// callers queue up instead of all waking at the same moment.
    last: Mutex<Option<Instant>>,
}

impl Pacer {
    fn new(gap: Duration) -> Self {
        Pacer {
            gap,
            last: Mutex::new(None),
        }
    }

    async fn wait(&self) {
        let mut last = self.last.lock().await;
        if let Some(previous) = *last {
            tokio::time::sleep_until(previous + self.gap).await;
        }
        *last = Some(Instant::now());
    }
}

impl HttpFetcher {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            // Identify ourselves honestly: providers like Gutenberg ask bots to.
            .user_agent(concat!(
                "sunlit-garden/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/aircode610/sunlit-garden)"
            ))
            .timeout(Duration::from_secs(120))
            .https_only(true)
            .build()?;
        Ok(HttpFetcher {
            client,
            pacer: Pacer::new(GAP),
        })
    }
}

impl Fetcher for HttpFetcher {
    async fn fetch(&self, url: &str) -> Result<Bytes, FetchError> {
        let err = |e: reqwest::Error| FetchError(e.to_string());
        self.pacer.wait().await;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(err)?
            .error_for_status()
            .map_err(err)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn requests_are_spaced_by_the_gap_and_the_first_goes_at_once() {
        let pacer = Pacer::new(Duration::from_secs(2));
        let start = Instant::now();

        pacer.wait().await;
        let first = start.elapsed();
        pacer.wait().await;
        pacer.wait().await;

        assert_eq!(first, Duration::ZERO);
        assert_eq!(start.elapsed(), Duration::from_secs(4));
    }
}
