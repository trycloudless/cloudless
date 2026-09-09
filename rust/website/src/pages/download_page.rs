use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use crate::releases::{DownloadMetadata, PlatformDownload};
use axum::{
    extract::Query,
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect},
};
use axum_extra::extract::cookie::CookieJar;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::{components::layout::Layout, error::WebsiteError};

const BETA_FORM_URL: &str = "https://docs.google.com/forms/d/e/1FAIpQLSdqLUGSONNSEaTJr-Gz9vPeLfaiw6r4UL-L_lLxnQok9fLG1A/viewform";

fn detect_os(user_agent: &str) -> &'static str {
    if user_agent.contains("Android") {
        "android"
    } else if user_agent.contains("iPhone") || user_agent.contains("iPad") {
        "ios"
    } else if user_agent.contains("Macintosh") || user_agent.contains("Mac OS X") {
        "macos"
    } else if user_agent.contains("Windows") {
        "windows"
    } else if user_agent.contains("Linux") {
        "linux"
    } else {
        "other"
    }
}

// ── Icons ────────────────────────────────────────────────────────────────────

#[component]
fn AppleIcon() -> impl IntoView {
    view! {
        <svg class="w-8 h-8" viewBox="0 0 24 24" fill="currentColor">
            <path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.8-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"/>
        </svg>
    }
}

#[component]
fn WindowsIcon() -> impl IntoView {
    view! {
        <svg class="w-8 h-8" viewBox="0 0 24 24" fill="currentColor">
            <path d="M3 12V6.75l6-1.32v6.48L3 12m17-9v8.75l-10 .15V5.21L20 3M3 13l6 .09v6.81l-6-1.15V13m17 .25V22l-10-1.91V13.1L20 13.25z"/>
        </svg>
    }
}

#[component]
fn LinuxIcon() -> impl IntoView {
    view! {
        <svg class="w-8 h-8" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12.504 0c-.155 0-.315.008-.48.021-4.226.333-3.105 4.807-3.17 6.298-.076 1.092-.3 1.953-1.05 3.02-.885 1.051-2.127 2.75-2.716 4.521-.278.832-.41 1.684-.287 2.489.117.779.567 1.563 1.182 2.114.542.485 1.174.785 1.658.794 1.26.025 2.47-.374 3.637-1.15 1.026-.678 2.095-1.8 2.7-2.702.602-.9 1.12-1.938 1.34-2.929.224-.996.226-2.042-.042-3.016-.276-.99-.82-1.892-1.48-2.624-.662-.734-1.396-1.286-1.916-1.713.204-.208.486-.441.77-.597.27-.148.534-.28.802-.424.73-.386 1.53-.797 2.195-1.46.67-.666 1.262-1.643 1.505-2.907C16.895 1.063 14.796 0 12.504 0zm.858 4.396a.88.88 0 0 1 .822.557c.148.361.107.788-.092 1.122-.196.333-.533.57-.923.637a.924.924 0 0 1-1.003-.604.867.867 0 0 1 .093-1.117.926.926 0 0 1 1.103-.595zM9.82 6.62a.895.895 0 0 1 .822.557c.15.361.108.788-.092 1.122-.197.333-.534.57-.924.637a.924.924 0 0 1-1.003-.604.866.866 0 0 1 .093-1.117.924.924 0 0 1 1.104-.595zm5.25 5.01c.564.04 1.182.438 1.615 1.217.595 1.07.62 2.667-.327 3.838-.942 1.169-2.26 1.654-3.416 1.86-1.155.206-2.254.146-3.02-.13-.765-.275-1.172-.724-1.158-1.207.015-.484.463-.985 1.086-1.275.624-.29 1.374-.347 2.116-.177.374.086.738.23 1.08.41.34.18.658.387.936.575.28.187.52.35.716.463.195.114.35.178.443.163.092-.015.158-.1.183-.254.027-.154.016-.387-.035-.662-.051-.275-.143-.595-.278-.925a5.424 5.424 0 0 0-.543-1.016c.283-.37.49-.777.602-1.88z"/>
        </svg>
    }
}

#[component]
fn AndroidIcon() -> impl IntoView {
    view! {
        <svg class="w-8 h-8" viewBox="0 0 24 24" fill="currentColor">
            <path d="M17.523 15.341a.95.95 0 0 1-.951.951.95.95 0 0 1-.951-.951V10.37a.95.95 0 0 1 .951-.952.95.95 0 0 1 .951.952v4.971m-11.047 0a.951.951 0 0 1-.951-.951V10.37a.951.951 0 0 1 1.903 0v4.971a.951.951 0 0 1-.952.951M6.67 8.469h10.66v7.6a1.045 1.045 0 0 1-1.046 1.046H7.716a1.046 1.046 0 0 1-1.046-1.046v-7.6M8.723 5.8l-.975-1.688a.2.2 0 0 1 .073-.273.2.2 0 0 1 .274.073l.988 1.711A6.38 6.38 0 0 1 12 5.075c.98 0 1.908.223 2.717.548l.988-1.711a.2.2 0 0 1 .274-.073.2.2 0 0 1 .073.273L15.077 5.8A5.93 5.93 0 0 1 18 8.469H6A5.93 5.93 0 0 1 8.723 5.8m2.329.952a.476.476 0 1 0 .001-.953.476.476 0 0 0-.001.953m2.896 0a.476.476 0 1 0 .001-.953.476.476 0 0 0-.001.953"/>
        </svg>
    }
}

#[component]
fn IosIcon() -> impl IntoView {
    view! {
        <svg class="w-8 h-8" viewBox="0 0 24 24" fill="currentColor">
            <path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.8-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"/>
        </svg>
    }
}

#[component]
fn DownloadIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
    }
}

// ── Platform card ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct PlatformInfo {
    key: &'static str,
    /// Maps to `?platform=` in `/api/download` — matches keys in `latest-download.json`.
    platform_key: &'static str,
    name: &'static str,
    subtitle: &'static str,
    format: &'static str,
    min_os: &'static str,
}

#[component]
fn PlatformCard(
    info: PlatformInfo,
    download: Option<PlatformDownload>,
    is_primary: bool,
) -> impl IntoView {
    let available = download.as_ref().map(|d| d.is_available()).unwrap_or(false);
    let download_href = format!("/api/download?platform={}", info.platform_key);
    let sha256 = download
        .as_ref()
        .map(|d| d.sha256.clone())
        .unwrap_or_default();
    let has_sha = !sha256.is_empty();

    let card_class = if is_primary {
        "glass bg-surface border border-primary-glow rounded-2xl p-6 flex flex-col relative"
    } else {
        "glass bg-surface border border-border rounded-2xl p-6 flex flex-col"
    };

    let icon_view = match info.key {
        "macos" => view! { <AppleIcon /> }.into_any(),
        "windows" => view! { <WindowsIcon /> }.into_any(),
        "linux" => view! { <LinuxIcon /> }.into_any(),
        "android" => view! { <AndroidIcon /> }.into_any(),
        "ios" => view! { <IosIcon /> }.into_any(),
        _ => view! { <span /> }.into_any(),
    };

    view! {
        <div class=card_class>
            {if is_primary {
                view! {
                    <div class="absolute -top-3 left-1/2 -translate-x-1/2 bg-primary text-white text-xs px-3 py-1 rounded-full font-semibold">
                        "Recommended"
                    </div>
                }.into_any()
            } else {
                view! { <span /> }.into_any()
            }}

            <div class="flex items-center gap-3 mb-4">
                <div class="text-text-secondary">{icon_view}</div>
                <div>
                    <div class="font-semibold text-text-primary">{info.name}</div>
                    <div class="text-xs text-text-secondary">{info.subtitle}</div>
                </div>
            </div>

            <div class="flex-1">
                <div class="text-xs text-text-secondary mb-1">
                    <span class="font-medium">"Format: "</span>{info.format}
                </div>
                <div class="text-xs text-text-secondary mb-4">
                    <span class="font-medium">"Requires: "</span>{info.min_os}
                </div>
            </div>

            {if available {
                view! {
                    <div class="space-y-2">
                        <a
                            href=download_href
                            class="flex items-center justify-center gap-2 w-full btn-gradient text-white py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                        >
                            <DownloadIcon />
                            "Download"
                        </a>
                        {if has_sha {
                            let sha_display = sha256.clone();
                            view! {
                                <div class="text-center space-y-1">
                                    <button
                                        class="text-xs text-text-secondary hover:text-text-primary transition-colors"
                                        onclick=format!("navigator.clipboard.writeText('{}').then(()=>this.textContent='Copied!').catch(()=>{{}})", sha256)
                                    >
                                        "Copy SHA-256"
                                    </button>
                                    <p class="font-mono text-xs text-text-secondary/60 break-all px-1">{sha_display}</p>
                                </div>
                            }.into_any()
                        } else {
                            view! { <span /> }.into_any()
                        }}
                    </div>
                }.into_any()
            } else {
                view! {
                    <button
                        disabled
                        class="w-full border border-border text-text-secondary/40 py-3 rounded-xl font-semibold cursor-not-allowed"
                    >
                        "Coming soon"
                    </button>
                }.into_any()
            }}
        </div>
    }
}

// ── Store card (mobile) ───────────────────────────────────────────────────────

#[component]
fn StoreCard(
    icon_key: &'static str,
    name: &'static str,
    store_name: &'static str,
    store_url: String,
) -> impl IntoView {
    let available = !store_url.is_empty();
    let icon = match icon_key {
        "android" => view! { <AndroidIcon /> }.into_any(),
        _ => view! { <IosIcon /> }.into_any(),
    };

    view! {
        <div class="glass bg-surface border border-border rounded-2xl p-6 flex flex-col items-center text-center gap-4">
            <div class="text-text-secondary">{icon}</div>
            <div>
                <div class="font-semibold text-text-primary">{name}</div>
                <div class="text-xs text-text-secondary mt-1">{store_name}</div>
            </div>
            {if available {
                view! {
                    <a
                        href=store_url
                        target="_blank"
                        rel="noopener noreferrer"
                        class="w-full text-center border border-primary text-primary-light py-2 rounded-xl font-semibold hover:bg-primary-tint transition-all text-sm"
                    >
                        "Get from store"
                    </a>
                }.into_any()
            } else {
                view! {
                    <span class="w-full text-center border border-border text-text-secondary/40 py-2 rounded-xl font-semibold text-sm cursor-not-allowed">
                        "Coming soon"
                    </span>
                }.into_any()
            }}
        </div>
    }
}

// ── Main page component ───────────────────────────────────────────────────────

#[component]
fn DownloadPage(metadata: Option<DownloadMetadata>, detected_os: String) -> impl IntoView {
    let version_label = metadata
        .as_ref()
        .map(|m| format!("v{}", m.version))
        .unwrap_or_else(|| "Coming soon".to_string());

    let release_date = metadata
        .as_ref()
        .map(|m| m.published_at.split('T').next().unwrap_or("").to_string())
        .unwrap_or_default();

    let release_notes_url = metadata
        .as_ref()
        .map(|m| m.release_notes_url.clone())
        .unwrap_or_default();

    let has_release_notes = !release_notes_url.is_empty();

    // Build platform info list in preferred order
    let platforms: Vec<(PlatformInfo, Option<PlatformDownload>)> = vec![
        (
            PlatformInfo {
                key: "macos",
                platform_key: "macos-aarch64",
                name: "macOS Apple Silicon",
                subtitle: "M1, M2, M3, M4",
                format: ".dmg",
                min_os: "macOS 11.0+",
            },
            metadata.as_ref().map(|m| m.downloads.macos_aarch64.clone()),
        ),
        (
            PlatformInfo {
                key: "macos",
                platform_key: "macos-x86_64",
                name: "macOS Intel",
                subtitle: "x86_64",
                format: ".dmg",
                min_os: "macOS 11.0+",
            },
            metadata.as_ref().map(|m| m.downloads.macos_x86_64.clone()),
        ),
        (
            PlatformInfo {
                key: "windows",
                platform_key: "windows-x86_64",
                name: "Windows",
                subtitle: "x64",
                format: ".msi",
                min_os: "Windows 10+",
            },
            metadata
                .as_ref()
                .map(|m| m.downloads.windows_x86_64.clone()),
        ),
        (
            PlatformInfo {
                key: "linux",
                platform_key: "linux-x86_64-appimage",
                name: "Linux AppImage",
                subtitle: "x86_64",
                format: ".AppImage",
                min_os: "Ubuntu 20.04+ / equivalent",
            },
            metadata
                .as_ref()
                .map(|m| m.downloads.linux_appimage.clone()),
        ),
        (
            PlatformInfo {
                key: "linux",
                platform_key: "linux-x86_64-deb",
                name: "Linux",
                subtitle: "x86_64 Debian/Ubuntu",
                format: ".deb",
                min_os: "Ubuntu 20.04+",
            },
            metadata.as_ref().map(|m| m.downloads.linux_deb.clone()),
        ),
    ];

    // Find the primary platform for the detected OS
    let primary_idx = match detected_os.as_str() {
        "windows" => 2,
        "linux" => 3,
        _ => 0, // macOS or unknown → Apple Silicon
    };

    let android_url = metadata
        .as_ref()
        .map(|m| m.mobile.android.store_url.clone())
        .unwrap_or_default();
    let ios_url = metadata
        .as_ref()
        .map(|m| m.mobile.ios.store_url.clone())
        .unwrap_or_default();

    view! {
        <div class="max-w-6xl mx-auto px-6 py-16">
            // Hero
            <div class="text-center mb-12">
                <div class="inline-flex items-center gap-2 bg-primary-tint border border-primary-ring rounded-full px-4 py-1.5 text-sm text-primary-light font-medium mb-6">
                    <span class="w-2 h-2 rounded-full bg-primary-light animate-pulse"></span>
                    {version_label}
                    {if !release_date.is_empty() {
                        view! {
                            <span class="text-text-secondary/60">" - "{release_date}</span>
                        }.into_any()
                    } else {
                        view! { <span /> }.into_any()
                    }}
                </div>
                <h1 class="text-4xl md:text-5xl font-display font-bold gradient-text mb-4">
                    "Download CloudLess"
                </h1>
                <p class="text-lg text-text-secondary max-w-xl mx-auto">
                    "Encrypted backup to S3, Google Drive, OneDrive, SFTP, or external storage. Private by design."
                </p>
            </div>

            // Feedback discount banner
            <div class="max-w-3xl mx-auto mb-12">
                <div class="glass bg-surface border border-primary-ring rounded-2xl px-5 py-4">
                    <div class="flex flex-col sm:flex-row items-center justify-center gap-1 text-center">
                        <p class="text-sm text-text-secondary">
                            "Try CloudLess. Share feedback. Get 3 months of Pro free."
                        </p>
                        <a
                            href=BETA_FORM_URL
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-sm text-primary-light font-semibold hover:text-text-primary transition-colors"
                        >
                            "Fill the form to avail offer."
                        </a>
                    </div>
                </div>
            </div>

            // Primary recommended download
            <div class="max-w-sm mx-auto mb-12">
                {
                    let (info, dl) = platforms[primary_idx].clone();
                    view! {
                        <PlatformCard info=info download=dl is_primary=true />
                    }
                }
            </div>

            // All desktop platforms
            <div class="mb-12">
                <h2 class="text-lg font-semibold text-text-secondary uppercase tracking-wider mb-6 text-center">
                    "Desktop downloads"
                </h2>
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5 gap-4">
                    {platforms
                        .into_iter()
                        .map(|(info, dl)| {
                            view! {
                                <PlatformCard info=info download=dl is_primary=false />
                            }
                        })
                        .collect_view()}
                </div>
            </div>

            // Mobile
            <div class="mb-12">
                <h2 class="text-lg font-semibold text-text-secondary uppercase tracking-wider mb-6 text-center">
                    "Mobile apps"
                </h2>
                <div class="grid grid-cols-1 sm:grid-cols-2 max-w-lg mx-auto gap-4">
                    <StoreCard
                        icon_key="android"
                        name="Android"
                        store_name="Google Play Store"
                        store_url=android_url
                    />
                    <StoreCard
                        icon_key="ios"
                        name="iOS"
                        store_name="Apple App Store"
                        store_url=ios_url
                    />
                </div>
            </div>

            // Footer links
            {if has_release_notes {
                view! {
                    <div class="text-center text-sm text-text-secondary">
                        <a
                            href=release_notes_url
                            class="hover:text-text-primary transition-colors"
                        >
                            "Release notes"
                        </a>
                    </div>
                }.into_any()
            } else {
                view! { <span /> }.into_any()
            }}
        </div>
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub(crate) struct DownloadQuery {
    platform: String,
}

/// Resolves a `?platform=<key>` query param to the real S3 URL and redirects.
/// Caddy logs this route, giving us per-platform download counts.
pub async fn download_redirect_handler(
    RequestEnv(env): RequestEnv,
    Query(params): Query<DownloadQuery>,
) -> Result<impl IntoResponse, WebsiteError> {
    let Some(meta) = env.releases().load(&env.http_api().client).await else {
        return Err(WebsiteError::NotFound(
            "release metadata unavailable".to_string(),
        ));
    };

    let download = match params.platform.as_str() {
        "macos-aarch64" => &meta.downloads.macos_aarch64,
        "macos-x86_64" => &meta.downloads.macos_x86_64,
        "windows-x86_64" => &meta.downloads.windows_x86_64,
        "linux-x86_64-appimage" => &meta.downloads.linux_appimage,
        "linux-x86_64-deb" => &meta.downloads.linux_deb,
        _ => {
            return Err(WebsiteError::NotFound(format!(
                "unknown platform: {}",
                params.platform
            )));
        }
    };

    if !download.is_available() {
        return Err(WebsiteError::NotFound(format!(
            "platform not available: {}",
            params.platform
        )));
    }

    tracing::info!(platform = %params.platform, version = %meta.version, "download redirect");

    Ok(Redirect::temporary(&download.url))
}

pub async fn download_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;

    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let detected_os = detect_os(user_agent).to_string();

    let metadata = env.releases().load(&env.http_api().client).await;

    let html = view! {
        <Layout title="Download - CloudLess".to_string() auth=state description="Download CloudLess for macOS, Windows, and Linux. Encrypted backup to storage you control.".to_string()>
            <DownloadPage metadata=metadata detected_os=detected_os />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
