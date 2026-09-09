use api_types::remote_storage::OneDriveCredentials;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use crate::error::{TauriError, TauriResult};

const AUTH_BASE: &str = "https://login.microsoftonline.com";
const TOKEN_BASE: &str = "https://login.microsoftonline.com";
const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";
const OIDC_USERINFO_ENDPOINT: &str = "https://graph.microsoft.com/oidc/userinfo";
const ONEDRIVE_SCOPE: &str = "openid profile email offline_access Files.ReadWrite.AppFolder";
const LOOPBACK_CALLBACK_PATH: &str = "/api/storage/onedrive/callback";

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct MicrosoftUserInfo {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
}

/// Runs the Microsoft OneDrive OAuth2 PKCE authorization flow.
///
/// Tokens are acquired in the Tauri backend and returned for immediate
/// client-side encryption; the CloudLess API server never receives plaintext
/// Microsoft tokens.
pub async fn onedrive_oauth_flow(
    app: &tauri::AppHandle,
    client_id: &str,
    tenant: &str,
    redirect_uri: &str,
) -> TauriResult<OneDriveCredentials> {
    let tenant = normalize_tenant(tenant);
    let redirect_uri = parse_loopback_redirect_uri(redirect_uri)?;
    let code_verifier = random_url_safe(64);
    let code_challenge = {
        let digest = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    };
    let state = random_url_safe(32);

    let bind_port = redirect_uri.registered_port.unwrap_or(0);
    let listener = TcpListener::bind(("127.0.0.1", bind_port))
        .map_err(|e| TauriError::from(format!("Failed to bind local listener: {}", e)))?;
    let runtime_port = listener.local_addr()?.port();
    let runtime_redirect_uri = redirect_uri.runtime_uri(runtime_port)?;

    let auth_url = build_authorization_url(
        AUTH_BASE,
        &tenant,
        client_id,
        &runtime_redirect_uri,
        &state,
        &code_challenge,
    );

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&auth_url, None::<&str>)
        .map_err(|e| TauriError::from(format!("Failed to open browser: {}", e)))?;

    let expected_state = state.clone();
    let (auth_code, _) = tokio::task::spawn_blocking(move || {
        wait_for_oauth_callback(listener, &expected_state, Duration::from_secs(300))
    })
    .await
    .map_err(|e| TauriError::from(format!("OAuth callback task failed: {}", e)))??;

    let http = reqwest::Client::new();
    let token_resp = http
        .post(format!(
            "{}/{}/oauth2/v2.0/token",
            TOKEN_BASE,
            urlencoded(&tenant)
        ))
        .form(&[
            ("client_id", client_id),
            ("code", auth_code.as_str()),
            ("redirect_uri", runtime_redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier.as_str()),
            ("scope", ONEDRIVE_SCOPE),
        ])
        .send()
        .await
        .map_err(|e| TauriError::from(format!("Token exchange request failed: {}", e)))?;

    if !token_resp.status().is_success() {
        let body = token_resp.text().await.unwrap_or_default();
        return Err(TauriError::from(format!(
            "Microsoft token exchange failed: {}",
            body
        )));
    }

    let tokens: TokenResponse = token_resp
        .json()
        .await
        .map_err(|e| TauriError::from(format!("Failed to parse token response: {}", e)))?;
    let refresh_token = tokens.refresh_token.ok_or_else(|| {
        TauriError::from("No refresh token in response. Ensure offline_access is requested.")
    })?;

    let user_info = fetch_microsoft_userinfo(&http, &tokens.access_token).await?;
    verify_onedrive_app_folder(&http, &tokens.access_token).await?;

    Ok(OneDriveCredentials {
        client_id: client_id.to_string(),
        tenant,
        refresh_token,
        microsoft_user_id: Some(user_info.sub),
        microsoft_user_email: user_info.email.or(user_info.preferred_username),
    })
}

struct LoopbackRedirectUri {
    scheme: String,
    host: String,
    path: String,
    registered_port: Option<u16>,
}

impl LoopbackRedirectUri {
    /// Builds the redirect URI used in the authorize request for the bound port.
    fn runtime_uri(&self, runtime_port: u16) -> TauriResult<String> {
        let port = self.registered_port.unwrap_or(runtime_port);
        Ok(format!(
            "{}://{}:{}{}",
            self.scheme, self.host, port, self.path
        ))
    }
}

/// Validates the registered OneDrive loopback redirect URI.
fn parse_loopback_redirect_uri(raw: &str) -> TauriResult<LoopbackRedirectUri> {
    let url = url::Url::parse(raw)
        .map_err(|e| TauriError::from(format!("Invalid ONEDRIVE_REDIRECT_URI: {}", e)))?;
    let host = url
        .host_str()
        .ok_or_else(|| TauriError::from("ONEDRIVE_REDIRECT_URI must include a host"))?;
    if url.scheme() != "http" || (host != "localhost" && host != "127.0.0.1") {
        return Err(TauriError::from(
            "ONEDRIVE_REDIRECT_URI must be an http localhost loopback URI.",
        ));
    }
    if url.path() != LOOPBACK_CALLBACK_PATH {
        return Err(TauriError::from(format!(
            "ONEDRIVE_REDIRECT_URI path must be {}",
            LOOPBACK_CALLBACK_PATH
        )));
    }

    Ok(LoopbackRedirectUri {
        scheme: url.scheme().to_string(),
        host: host.to_string(),
        path: url.path().to_string(),
        registered_port: url.port(),
    })
}

/// Builds the Microsoft authorization URL with PKCE parameters.
fn build_authorization_url(
    auth_base: &str,
    tenant: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    format!(
        "{}/{}/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}\
         &response_mode=query&scope={}&state={}&code_challenge={}&code_challenge_method=S256\
         &prompt=select_account",
        auth_base.trim_end_matches('/'),
        urlencoded(tenant),
        urlencoded(client_id),
        urlencoded(redirect_uri),
        urlencoded(ONEDRIVE_SCOPE),
        urlencoded(state),
        urlencoded(code_challenge),
    )
}

/// Waits for the loopback OAuth redirect and validates the CSRF state.
fn wait_for_oauth_callback(
    listener: TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> TauriResult<(String, String)> {
    listener
        .set_nonblocking(true)
        .map_err(|e| TauriError::from(e.to_string()))?;
    let start = Instant::now();

    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > timeout {
                    return Err(TauriError::from(
                        "Timed out waiting for Microsoft OAuth callback",
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(TauriError::from(format!("Accept failed: {}", e))),
        }
    };

    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| TauriError::from(e.to_string()))?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| TauriError::from(format!("Failed to read request: {}", e)))?;

    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| TauriError::from("Invalid HTTP request"))?
        .to_owned();

    let callback = parse_callback_path(&path, expected_state);
    write_callback_response(&stream, callback.is_ok())?;
    stream.shutdown(std::net::Shutdown::Both).ok();
    callback
}

/// Parses the OAuth callback path and returns the authorization code.
fn parse_callback_path(path: &str, expected_state: &str) -> TauriResult<(String, String)> {
    let query = path
        .split('?')
        .nth(1)
        .ok_or_else(|| TauriError::from("No query parameters in callback"))?;

    let mut code = None;
    let mut received_state = None;
    let mut error = None;

    for param in query.split('&') {
        let mut kv = param.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("code"), Some(v)) => code = Some(urldecoded(v)),
            (Some("state"), Some(v)) => received_state = Some(urldecoded(v)),
            (Some("error"), Some(v)) => error = Some(urldecoded(v)),
            _ => {}
        }
    }

    if let Some(error) = error {
        return Err(TauriError::from(format!(
            "Microsoft authorization denied: {}",
            error
        )));
    }

    let code = code.ok_or_else(|| TauriError::from("No authorization code in callback"))?;
    let received_state = received_state.ok_or_else(|| TauriError::from("No state in callback"))?;
    if received_state != expected_state {
        return Err(TauriError::from("OAuth state mismatch — possible CSRF"));
    }

    Ok((code, received_state))
}

/// Writes a small browser response so the user can return to CloudLess.
fn write_callback_response(mut stream: &std::net::TcpStream, success: bool) -> TauriResult<()> {
    let html = if success {
        "<html><body><h2>Authorization successful!</h2><p>You can close this tab and return to CloudLess.</p></body></html>"
    } else {
        "<html><body><h2>Authorization failed</h2><p>Return to CloudLess and try again.</p></body></html>"
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| TauriError::from(format!("Failed to send response: {}", e)))
}

/// Fetches the Microsoft account identity used for reauthorization checks.
async fn fetch_microsoft_userinfo(
    http: &reqwest::Client,
    access_token: &str,
) -> TauriResult<MicrosoftUserInfo> {
    let resp = http
        .get(OIDC_USERINFO_ENDPOINT)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| TauriError::from(format!("Failed to fetch Microsoft user info: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(TauriError::from(format!(
            "Microsoft user info request failed: {}",
            body
        )));
    }

    resp.json::<MicrosoftUserInfo>()
        .await
        .map_err(|e| TauriError::from(format!("Failed to parse Microsoft user info: {}", e)))
}

/// Verifies that the token can access and create the OneDrive app folder.
async fn verify_onedrive_app_folder(http: &reqwest::Client, access_token: &str) -> TauriResult<()> {
    let resp = http
        .get(format!("{}/me/drive/special/approot", GRAPH_API))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| TauriError::from(format!("Failed to verify OneDrive app folder: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(TauriError::from(format!(
            "OneDrive app folder verification failed: {}",
            body
        )));
    }
    Ok(())
}

/// Generates a cryptographically random URL-safe string.
fn random_url_safe(len: usize) -> String {
    let bytes: Vec<u8> = rand::rng().random_iter::<u8>().take(len).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Normalizes blank tenant config to the public Microsoft `common` authority.
fn normalize_tenant(tenant: &str) -> String {
    let trimmed = tenant.trim();
    if trimmed.is_empty() {
        "common".to_string()
    } else {
        trimmed.to_string()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensures the authorization URL uses `common`, PKCE, app-folder scope, and no secret.
    #[test]
    fn authorization_url_contains_required_parameters() {
        let url = build_authorization_url(
            "https://login.microsoftonline.com",
            "common",
            "client-id",
            "http://localhost:1234/api/storage/onedrive/callback",
            "state",
            "challenge",
        );

        assert!(url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"));
        assert!(url.contains("client_id=client-id"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("Files.ReadWrite.AppFolder"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(!url.contains("client_secret"));
    }

    /// Ensures callback parsing rejects mismatched state values before token exchange.
    #[test]
    fn callback_parser_rejects_state_mismatch() {
        let result = parse_callback_path("/callback?code=abc&state=wrong", "expected");

        assert!(result.is_err());
    }

    /// Ensures callback parsing returns the code when Microsoft redirects successfully.
    #[test]
    fn callback_parser_returns_authorization_code() {
        let (code, state) =
            parse_callback_path("/callback?code=abc123&state=expected", "expected").unwrap();

        assert_eq!(code, "abc123");
        assert_eq!(state, "expected");
    }

    /// Ensures exact-port local redirect config binds and sends that port.
    #[test]
    fn loopback_redirect_uri_supports_explicit_port() {
        let parsed =
            parse_loopback_redirect_uri("http://localhost:3000/api/storage/onedrive/callback")
                .unwrap();

        assert_eq!(parsed.registered_port, Some(3000));
        assert_eq!(
            parsed.runtime_uri(53142).unwrap(),
            "http://localhost:3000/api/storage/onedrive/callback"
        );
        assert!(parse_loopback_redirect_uri("http://localhost/callback").is_err());
    }

    /// Ensures production redirect config can use an OS-assigned free port.
    #[test]
    fn loopback_redirect_uri_supports_ephemeral_port() {
        let parsed =
            parse_loopback_redirect_uri("http://localhost/api/storage/onedrive/callback").unwrap();

        assert_eq!(parsed.registered_port, None);
        assert_eq!(
            parsed.runtime_uri(53142).unwrap(),
            "http://localhost:53142/api/storage/onedrive/callback"
        );
    }

    /// Ensures the OIDC UserInfo response provides the stable account subject.
    #[test]
    fn microsoft_userinfo_parses_oidc_claims() {
        let json = r#"{
            "sub": "account-subject",
            "email": "person@example.com",
            "preferred_username": "person@example.com"
        }"#;

        let info: MicrosoftUserInfo = serde_json::from_str(json).unwrap();

        assert_eq!(info.sub, "account-subject");
        assert_eq!(info.email.as_deref(), Some("person@example.com"));
    }
}
