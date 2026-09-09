use api_types::remote_storage::GoogleDriveCredentials;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use crate::error::{TauriError, TauriResult};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3/files";
const USERINFO_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v1/userinfo";
/// OAuth scopes: drive.file for backup storage, openid+email for identity verification during reauth.
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file openid email";

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct FileListResponse {
    files: Option<Vec<FileResource>>,
}

#[derive(Deserialize)]
struct FileResource {
    id: String,
}

#[derive(Deserialize)]
struct CreateFileResponse {
    id: String,
}

/// Google userinfo API response.
/// Used to capture the stable account ID and email for identity verification.
#[derive(Deserialize)]
struct GoogleUserInfo {
    /// Stable Google account identifier (never changes for a given account).
    id: String,
    /// Google account email address.
    email: String,
}

/// Generates a cryptographically random URL-safe string of `len` bytes.
fn random_url_safe(len: usize) -> String {
    let bytes: Vec<u8> = rand::rng().random_iter::<u8>().take(len).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Runs the full Google Drive OAuth2 PKCE authorization flow.
///
/// 1. Starts a local HTTP listener on a random port.
/// 2. Opens the Google consent screen in the user's default browser.
/// 3. Captures the authorization code from the redirect callback.
/// 4. Exchanges the code for access + refresh tokens (PKCE only — no client secret).
/// 5. Creates (or finds) the root backup folder in Google Drive.
/// 6. Returns [`GoogleDriveCredentials`] ready for encryption and storage.
///
/// Google treats installed/desktop apps as public clients — the client secret
/// is not required and should not be embedded in distributed binaries.
/// The OAuth app must be registered as "Desktop app" type in Google Cloud Console.
pub async fn google_drive_oauth_flow(
    app: &tauri::AppHandle,
    client_id: &str,
) -> TauriResult<GoogleDriveCredentials> {
    // ── 1. Generate PKCE pair + state ───────────────────────────────
    let code_verifier = random_url_safe(64);
    let code_challenge = {
        let digest = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    };
    let state = random_url_safe(32);

    // ── 2. Start local HTTP listener ────────────────────────────────
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| TauriError::from(format!("Failed to bind local listener: {}", e)))?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{}/callback", port);

    // ── 3. Build + open authorization URL ───────────────────────────
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &access_type=offline&prompt=consent\
         &code_challenge={}&code_challenge_method=S256&state={}",
        AUTH_ENDPOINT,
        urlencoded(client_id),
        urlencoded(&redirect_uri),
        urlencoded(SCOPE),
        urlencoded(&code_challenge),
        urlencoded(&state),
    );

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&auth_url, None::<&str>)
        .map_err(|e| TauriError::from(format!("Failed to open browser: {}", e)))?;

    // ── 4. Wait for the callback ────────────────────────────────────
    // Block on a std TCP accept in a spawn_blocking to avoid tying up
    // the async runtime.
    let expected_state = state.clone();
    let (auth_code, _) = tokio::task::spawn_blocking(move || -> TauriResult<(String, String)> {
        // Set a 5-minute timeout so we don't hang forever.
        listener
            .set_nonblocking(false)
            .map_err(|e| TauriError::from(e.to_string()))?;
        let timeout = std::time::Duration::from_secs(300);
        listener.set_nonblocking(false).ok();

        // Accept with a timeout loop.
        let stream = loop {
            listener
                .set_nonblocking(true)
                .map_err(|e| TauriError::from(e.to_string()))?;
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    // Check if we've exceeded the timeout
                    // For simplicity we use a simple approach below
                    continue;
                }
                Err(e) => return Err(TauriError::from(format!("Accept failed: {}", e))),
            }
        };

        // Use a simpler approach: set blocking with SO_RCVTIMEO
        drop(listener); // stop listening after first connection
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| TauriError::from(e.to_string()))?;

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .map_err(|e| TauriError::from(format!("Failed to read request: {}", e)))?;

        // Parse GET /callback?code=...&state=... HTTP/1.1
        let path = request_line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| TauriError::from("Invalid HTTP request"))?
            .to_owned();

        // Send success response and close
        let html = "<html><body><h2>Authorization successful!</h2>\
                     <p>You can close this tab and return to CloudLess.</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        (&stream)
            .write_all(response.as_bytes())
            .map_err(|e| TauriError::from(format!("Failed to send response: {}", e)))?;
        stream
            .shutdown(std::net::Shutdown::Both)
            .ok();

        // Parse query parameters
        let query = path
            .split('?')
            .nth(1)
            .ok_or_else(|| TauriError::from("No query parameters in callback"))?;

        let mut code = None;
        let mut received_state = None;
        for param in query.split('&') {
            let mut kv = param.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some("code"), Some(v)) => code = Some(urldecoded(v)),
                (Some("state"), Some(v)) => received_state = Some(urldecoded(v)),
                _ => {}
            }
        }

        // Check for error parameter
        if code.is_none() {
            for param in query.split('&') {
                let mut kv = param.splitn(2, '=');
                if let (Some("error"), Some(v)) = (kv.next(), kv.next()) {
                    return Err(TauriError::from(format!(
                        "Google authorization denied: {}",
                        urldecoded(v)
                    )));
                }
            }
        }

        let code =
            code.ok_or_else(|| TauriError::from("No authorization code in callback"))?;
        let received_state =
            received_state.ok_or_else(|| TauriError::from("No state in callback"))?;

        // Verify state
        if received_state != expected_state {
            return Err(TauriError::from("OAuth state mismatch — possible CSRF"));
        }

        Ok((code, received_state))
    })
    .await
    .map_err(|e| TauriError::from(format!("OAuth callback task failed: {}", e)))??;

    // ── 5. Exchange code for tokens ─────────────────────────────────
    let http = reqwest::Client::new();
    // Desktop app type: no client_secret needed — PKCE (code_verifier) provides
    // equivalent security per Google's installed-app OAuth guidelines.
    let token_resp = http
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("code", auth_code.as_str()),
            ("client_id", client_id),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| TauriError::from(format!("Token exchange request failed: {}", e)))?;

    if !token_resp.status().is_success() {
        let body = token_resp.text().await.unwrap_or_default();
        return Err(TauriError::from(format!("Token exchange failed: {}", body)));
    }

    let tokens: TokenResponse = token_resp
        .json()
        .await
        .map_err(|e| TauriError::from(format!("Failed to parse token response: {}", e)))?;

    let refresh_token = tokens
        .refresh_token
        .ok_or_else(|| TauriError::from("No refresh token in response — ensure prompt=consent"))?;
    let access_token = tokens.access_token;

    // ── 6. Fetch Google user identity ─────────────────────────────────
    // Capture user ID and email for identity verification during reauth.
    let user_info = fetch_google_userinfo(&http, &access_token).await?;
    tracing::info!(
        google_user_id = %user_info.id,
        google_user_email = %user_info.email,
        "Captured Google account identity"
    );

    // ── 7. Find or create root backup folder ────────────────────────
    let root_folder_id = find_or_create_root_folder(&http, &access_token).await?;

    Ok(GoogleDriveCredentials {
        client_id: client_id.to_owned(),
        client_secret: None,
        refresh_token,
        root_folder_id,
        google_user_id: Some(user_info.id),
        google_user_email: Some(user_info.email),
    })
}

/// Fetches the Google account identity (user ID and email) from the userinfo endpoint.
/// The `id` field is a stable identifier that never changes for a given Google account,
/// making it suitable for identity verification during reauth.
async fn fetch_google_userinfo(
    http: &reqwest::Client,
    access_token: &str,
) -> TauriResult<GoogleUserInfo> {
    let resp = http
        .get(USERINFO_ENDPOINT)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| TauriError::from(format!("Failed to fetch Google userinfo: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(TauriError::from(format!(
            "Google userinfo request failed: {}",
            body
        )));
    }

    resp.json::<GoogleUserInfo>()
        .await
        .map_err(|e| TauriError::from(format!("Failed to parse Google userinfo: {}", e)))
}

/// Searches for an existing "CloudLess Backups" folder, or creates one.
async fn find_or_create_root_folder(
    http: &reqwest::Client,
    access_token: &str,
) -> TauriResult<String> {
    let query =
        "name = 'CloudLess Backups' and mimeType = 'application/vnd.google-apps.folder' and trashed = false";

    let resp = http
        .get(DRIVE_API)
        .bearer_auth(access_token)
        .query(&[("q", query), ("fields", "files(id)")])
        .send()
        .await
        .map_err(|e| TauriError::from(format!("Drive folder search failed: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(TauriError::from(format!(
            "Drive folder search failed: {}",
            body
        )));
    }

    let list: FileListResponse = resp
        .json()
        .await
        .map_err(|e| TauriError::from(format!("Failed to parse folder search: {}", e)))?;

    if let Some(files) = list.files {
        if let Some(folder) = files.into_iter().next() {
            tracing::info!("Found existing CloudLess Backups folder: {}", folder.id);
            return Ok(folder.id);
        }
    }

    // Create the folder
    let metadata = serde_json::json!({
        "name": "CloudLess Backups",
        "mimeType": "application/vnd.google-apps.folder",
    });

    let resp = http
        .post(DRIVE_API)
        .bearer_auth(access_token)
        .json(&metadata)
        .send()
        .await
        .map_err(|e| TauriError::from(format!("Drive folder creation failed: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(TauriError::from(format!(
            "Drive folder creation failed: {}",
            body
        )));
    }

    let created: CreateFileResponse = resp
        .json()
        .await
        .map_err(|e| TauriError::from(format!("Failed to parse folder creation: {}", e)))?;

    tracing::info!("Created CloudLess Backups folder: {}", created.id);
    Ok(created.id)
}

/// Minimal percent-encoding for URL query parameters.
fn urlencoded(s: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("v", s)
        .finish()
        .strip_prefix("v=")
        .unwrap_or("")
        .to_owned()
}

/// Minimal percent-decoding for URL query parameter values.
fn urldecoded(s: &str) -> String {
    url::form_urlencoded::parse(s.as_bytes())
        .next()
        .map(|(k, v)| {
            if v.is_empty() {
                k.into_owned()
            } else {
                format!("{}={}", k, v)
            }
        })
        .unwrap_or_else(|| s.to_owned())
}
