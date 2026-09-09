use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlatformDownload {
    #[allow(dead_code)]
    pub label: String,
    pub url: String,
    pub sha256: String,
}

impl PlatformDownload {
    pub fn is_available(&self) -> bool {
        !self.url.is_empty()
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MobileStore {
    pub store_url: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MobileDownloads {
    pub android: MobileStore,
    pub ios: MobileStore,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DesktopDownloads {
    #[serde(rename = "macos-aarch64")]
    pub macos_aarch64: PlatformDownload,
    #[serde(rename = "macos-x86_64")]
    pub macos_x86_64: PlatformDownload,
    #[serde(rename = "windows-x86_64")]
    pub windows_x86_64: PlatformDownload,
    #[serde(rename = "linux-x86_64-appimage")]
    pub linux_appimage: PlatformDownload,
    #[serde(rename = "linux-x86_64-deb")]
    pub linux_deb: PlatformDownload,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DownloadMetadata {
    pub version: String,
    pub published_at: String,
    pub release_notes_url: String,
    pub downloads: DesktopDownloads,
    pub mobile: MobileDownloads,
}

/// Shared, cloneable cache for release download metadata fetched from S3.
/// All clones share the same underlying cache via `Arc`.
#[derive(Clone)]
pub struct ReleaseMetadataCache {
    inner: Arc<RwLock<Option<(Instant, DownloadMetadata)>>>,
    url: Option<String>,
}

impl ReleaseMetadataCache {
    pub fn new(url: Option<String>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            url,
        }
    }

    /// Returns cached metadata if fresh, otherwise fetches from S3 and updates the cache.
    /// Returns `None` if the URL is not configured or the fetch fails.
    pub async fn load(&self, client: &reqwest::Client) -> Option<DownloadMetadata> {
        let url = self.url.as_deref()?;

        {
            let guard = self.inner.read().await;
            if let Some((fetched_at, ref meta)) = *guard {
                if fetched_at.elapsed() < CACHE_TTL {
                    return Some(meta.clone());
                }
            }
        }

        let resp = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to fetch release metadata from {}: {}", url, e);
                return None;
            }
        };

        if !resp.status().is_success() {
            tracing::warn!("Release metadata fetch returned {}: {}", resp.status(), url);
            return None;
        }

        let meta: DownloadMetadata = match resp.json().await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to parse release metadata JSON: {}", e);
                return None;
            }
        };

        let mut guard = self.inner.write().await;
        *guard = Some((Instant::now(), meta.clone()));

        Some(meta)
    }
}
