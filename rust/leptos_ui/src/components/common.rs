use api_types::local_device::DeviceSummary;
use api_types::remote_storage::RemoteStorageType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Typed error returned by all `tauri_invoke*` helpers.
/// Mirrors `TauriError` from the Tauri crate (same serde tags).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum TauriClientError {
    NotFound { message: String },
    Unauthorized { message: String },
    Validation { message: String },
    Internal { message: String },
}

impl fmt::Display for TauriClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NotFound { message }
            | Self::Unauthorized { message }
            | Self::Validation { message }
            | Self::Internal { message } => message,
        };
        write!(f, "{}", msg)
    }
}

impl TauriClientError {
    /// User-facing message. Internal errors (network failures, serialisation
    /// issues, unexpected panics) show a generic string instead of raw technical
    /// details; all other variants expose the server-supplied message directly.
    pub fn user_message(&self) -> &str {
        match self {
            Self::Internal { .. } => {
                "Something went wrong. Please check your connection and try again."
            }
            Self::NotFound { message }
            | Self::Unauthorized { message }
            | Self::Validation { message } => message,
        }
    }
}

fn parse_tauri_error(e: JsValue) -> TauriClientError {
    serde_wasm_bindgen::from_value::<TauriClientError>(e.clone()).unwrap_or_else(|_| {
        TauriClientError::Internal {
            message: e.as_string().unwrap_or_else(|| format!("{:?}", e)),
        }
    })
}

/// Mirrors the `DeviceResolutionResult` enum from tauri/src/models.rs.
/// Used by both DeviceRegistration and UnlockForm to deserialize the
/// result of the `resolve_mobile_device` Tauri command.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum DeviceResolutionResult {
    Resolved { device: DeviceSummary },
    MultipleFound { devices: Vec<DeviceSummary> },
    NoneFound,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>)
    -> Result<JsValue, JsValue>;
}

pub async fn tauri_invoke<T: Serialize, R: for<'de> Deserialize<'de>>(
    cmd: &str,
    arg_name: &str,
    args: T,
) -> Result<R, TauriClientError> {
    let obj = js_sys::Object::new();
    let value = serde_wasm_bindgen::to_value(&args).map_err(|e| TauriClientError::Internal {
        message: e.to_string(),
    })?;
    js_sys::Reflect::set(&obj, &JsValue::from_str(arg_name), &value).map_err(|e| {
        TauriClientError::Internal {
            message: format!("{:?}", e),
        }
    })?;

    match invoke(cmd, obj.into()).await {
        Ok(res) => serde_wasm_bindgen::from_value(res).map_err(|e| TauriClientError::Internal {
            message: e.to_string(),
        }),
        Err(e) => Err(parse_tauri_error(e)),
    }
}

/// Invoke a Tauri command with a pre-built serde-serializable args object.
/// The args value is serialized as the full JS object passed to invoke.
pub async fn tauri_invoke_with_args<A: Serialize, R: for<'de> Deserialize<'de>>(
    cmd: &str,
    args: &A,
) -> Result<R, TauriClientError> {
    let obj = serde_wasm_bindgen::to_value(args).map_err(|e| TauriClientError::Internal {
        message: e.to_string(),
    })?;
    match invoke(cmd, obj).await {
        Ok(res) => serde_wasm_bindgen::from_value(res).map_err(|e| TauriClientError::Internal {
            message: e.to_string(),
        }),
        Err(e) => Err(parse_tauri_error(e)),
    }
}

pub async fn tauri_invoke_no_args<R: for<'de> Deserialize<'de>>(
    cmd: &str,
) -> Result<R, TauriClientError> {
    let obj = js_sys::Object::new();
    match invoke(cmd, obj.into()).await {
        Ok(res) => serde_wasm_bindgen::from_value(res).map_err(|e| TauriClientError::Internal {
            message: e.to_string(),
        }),
        Err(e) => Err(parse_tauri_error(e)),
    }
}

/// Returns the user's local UTC offset in seconds east, obtained from JS `Date`.
fn local_utc_offset_secs() -> i32 {
    // JS getTimezoneOffset() returns minutes *west* of UTC (e.g. UTC-5 → 300).
    // chrono FixedOffset::east_opt expects seconds *east* of UTC.
    let js_offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i32; // west of UTC
    -js_offset_minutes * 60 // convert to east-of-UTC seconds
}

/// Formats a `DateTime<Utc>` as a date string (`YYYY-MM-DD`) in the user's local timezone.
pub fn format_local_date(dt: &DateTime<Utc>) -> String {
    let offset = chrono::FixedOffset::east_opt(local_utc_offset_secs())
        .unwrap_or(chrono::FixedOffset::east_opt(0).unwrap());
    dt.with_timezone(&offset).format("%Y-%m-%d").to_string()
}

/// Reads the user's 12h/24h preference from localStorage.
fn is_24h_format() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|ls| ls.get_item("date_format_24h").ok().flatten())
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// Formats a `DateTime<Utc>` as a date+time string in the user's local timezone.
/// Respects the 12h/24h preference stored in localStorage.
pub fn format_local_datetime(dt: &DateTime<Utc>) -> String {
    let offset = chrono::FixedOffset::east_opt(local_utc_offset_secs())
        .unwrap_or(chrono::FixedOffset::east_opt(0).unwrap());
    let local = dt.with_timezone(&offset);
    if is_24h_format() {
        local.format("%Y-%m-%d %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d %I:%M %p").to_string()
    }
}

/// Formats a `DateTime<Utc>` as a date+time+seconds string in the user's local timezone.
/// Respects the 12h/24h preference stored in localStorage.
pub fn format_local_datetime_sec(dt: &DateTime<Utc>) -> String {
    let offset = chrono::FixedOffset::east_opt(local_utc_offset_secs())
        .unwrap_or(chrono::FixedOffset::east_opt(0).unwrap());
    let local = dt.with_timezone(&offset);
    if is_24h_format() {
        local.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        local.format("%Y-%m-%d %I:%M:%S %p").to_string()
    }
}

/// Returns a human-readable relative time string, e.g. "just now", "5 minutes ago", "2 hours ago", "yesterday".
pub fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);
    let secs = duration.num_seconds();

    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        let mins = duration.num_minutes();
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if secs < 86400 {
        let hours = duration.num_hours();
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if secs < 172800 {
        "yesterday".to_string()
    } else if secs < 604800 {
        format!("{} days ago", duration.num_days())
    } else {
        format_local_datetime(dt)
    }
}

pub fn tauri_listen_event<R, F>(event_name: &'static str, callback: F)
where
    R: for<'de> Deserialize<'de> + 'static,
    F: Fn(R) + 'static,
{
    spawn_local(async move {
        let cb = Closure::new(move |val: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&val, &JsValue::from_str("payload")) {
                if let Ok(data) = serde_wasm_bindgen::from_value::<R>(payload) {
                    callback(data);
                }
            }
        });
        let _ = listen(event_name, &cb).await;
        cb.forget();
    });
}

// ── Shared UI Helpers ──────────────────────────────────────────────

/// Formats byte count into human-readable string (delegates to shared_ui).
pub fn format_bytes(bytes: u64) -> String {
    shared_ui::utils::format_bytes(bytes)
}

/// Reads a value from localStorage, returning `None` if absent or unavailable.
pub fn read_local_storage(key: &str) -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|ls| ls.get_item(key).ok().flatten())
}

/// Writes a value to localStorage. Silently no-ops if localStorage is unavailable.
pub fn write_local_storage(key: &str, value: &str) {
    if let Some(ls) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = ls.set_item(key, value);
    }
}

/// Triggers a download of `content` as a `.txt` file named `filename` in the WebView.
pub fn download_text_file(filename: &str, content: &str) {
    use wasm_bindgen::JsCast;
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let parts = js_sys::Array::new();
    parts.push(&js_sys::JsString::from(content));
    let Ok(blob) = web_sys::Blob::new_with_str_sequence(&parts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    let Ok(el) = document.create_element("a") else {
        return;
    };
    let Ok(a) = el.dyn_into::<web_sys::HtmlAnchorElement>() else {
        return;
    };
    a.set_href(&url);
    a.set_download(filename);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// Returns the filename (last path segment) from an absolute or relative path.
pub fn file_basename(path: &str) -> String {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Abbreviates a filesystem path by replacing `/Users/<name>/…` or `/home/<name>/…` with `~/…`.
/// Falls back to showing the last 2 path components for unrecognised root prefixes.
pub fn abbrev_home_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    for prefix in &["/Users/", "/home/"] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            return if let Some(slash) = rest.find('/') {
                format!("~/{}", &rest[slash + 1..])
            } else {
                "~/".to_string()
            };
        }
    }
    short_path(&normalized)
}

/// Shortens a path to show at most the last 2 components (for mobile display).
pub fn short_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.rsplit('/').take(2).collect();
    match parts.len() {
        0 => path.to_string(),
        1 => parts[0].to_string(),
        _ => format!(".../{}/{}", parts[1], parts[0]),
    }
}

/// Returns an inline SVG icon for the given remote storage type.
pub fn storage_type_icon(storage_type: &RemoteStorageType) -> &'static str {
    match storage_type {
        RemoteStorageType::Aws => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/><path d="M3 12c0 1.66 4.03 3 9 3s9-1.34 9-3"/></svg>"#
        }
        RemoteStorageType::GoogleDrive => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L2 19.5h7.5L12 14l2.5 5.5H22L12 2z"/><path d="M2 19.5h20"/><path d="M7.5 12L12 2l4.5 10"/></svg>"#
        }
        RemoteStorageType::OneDrive => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 18.5h9.5a4.5 4.5 0 10-1.08-8.87A6 6 0 004 11.5a3.5 3.5 0 003 7z"/><path d="M8.5 13.5h7"/><path d="M12 10v7"/></svg>"#
        }
        RemoteStorageType::LocalFilesystem => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12H2"/><path d="M5.45 5.11L2 12v6a2 2 0 002 2h16a2 2 0 002-2v-6l-3.45-6.89A2 2 0 0016.76 4H7.24a2 2 0 00-1.79 1.11z"/><line x1="6" y1="16" x2="6.01" y2="16"/><line x1="10" y1="16" x2="10.01" y2="16"/></svg>"#
        }
        RemoteStorageType::Sftp => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="14" rx="2"/><path d="M7 20h10"/><path d="M12 18v2"/><path d="M7 8h.01"/><path d="M10 8h.01"/><path d="M13 8h4"/><path d="M7 12h10"/></svg>"#
        }
    }
}

/// Returns an inline SVG icon for a file based on its extension.
pub fn file_type_icon(filename: &str) -> &'static str {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => {
            r#"<svg class="w-4 h-4 text-red-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>"#
        }
        "jpg" | "jpeg" | "png" | "gif" | "svg" | "webp" | "bmp" | "ico" | "heic" => {
            r#"<svg class="w-4 h-4 text-blue-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>"#
        }
        "mp4" | "mov" | "avi" | "mkv" | "webm" => {
            r#"<svg class="w-4 h-4 text-purple-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"/><line x1="7" y1="2" x2="7" y2="22"/><line x1="17" y1="2" x2="17" y2="22"/><line x1="2" y1="12" x2="22" y2="12"/><line x1="2" y1="7" x2="7" y2="7"/><line x1="2" y1="17" x2="7" y2="17"/><line x1="17" y1="7" x2="22" y2="7"/><line x1="17" y1="17" x2="22" y2="17"/></svg>"#
        }
        "mp3" | "wav" | "flac" | "aac" | "ogg" => {
            r#"<svg class="w-4 h-4 text-pink-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>"#
        }
        "zip" | "tar" | "gz" | "rar" | "7z" | "bz2" | "xz" => {
            r#"<svg class="w-4 h-4 text-yellow-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg>"#
        }
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
        | "html" | "css" | "scss" | "sh" | "rb" | "swift" | "kt" => {
            r#"<svg class="w-4 h-4 text-green-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"#
        }
        "json" | "yaml" | "yml" | "toml" | "xml" | "csv" | "ini" | "env" => {
            r#"<svg class="w-4 h-4 text-cyan-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><circle cx="10" cy="13" r="2"/><path d="M20 17l-2-2"/></svg>"#
        }
        "doc" | "docx" | "txt" | "rtf" | "md" | "odt" => {
            r#"<svg class="w-4 h-4 text-blue-300 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><line x1="10" y1="9" x2="8" y2="9"/></svg>"#
        }
        "xls" | "xlsx" | "ods" => {
            r#"<svg class="w-4 h-4 text-emerald-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><rect x="8" y="12" width="8" height="6"/><line x1="12" y1="12" x2="12" y2="18"/><line x1="8" y1="15" x2="16" y2="15"/></svg>"#
        }
        "ppt" | "pptx" | "odp" => {
            r#"<svg class="w-4 h-4 text-orange-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><rect x="8" y="12" width="8" height="6" rx="1"/></svg>"#
        }
        _ => {
            r#"<svg class="w-4 h-4 text-text-secondary flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>"#
        }
    }
}
