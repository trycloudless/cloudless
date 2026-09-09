use axum::{
    extract::{FromRef, FromRequestParts, Request, State},
    http::{self, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cloudless_core::ports::api::user_api_port::UserApiPort;
use std::time::{SystemTime, UNIX_EPOCH};

use api_types::user::UserRole;

use crate::env::WebsiteEnv;
use crate::state::WebsiteState;

/// Authentication info extracted from cookies for **UI rendering** (nav, layout).
///
/// `is_admin` comes from the client-writable `mvs_role` cookie and MUST NOT be
/// used for access-control decisions. Use [`verify_auth`] instead.
pub struct AuthInfo {
    pub logged_in: bool,
    pub token: Option<String>,
}

pub fn extract_auth(jar: &CookieJar) -> AuthInfo {
    let token = jar.get("mvs_token").map(|c| c.value().to_string());
    let logged_in = token.is_some();
    AuthInfo { logged_in, token }
}

/// Authenticated session state, used as a single prop for layout/nav rendering.
///
/// `Anonymous` — no valid session. `Authenticated` — verified against the API.
/// Always build with [`verify_auth`]; never construct directly from cookies.
#[derive(Clone, Default)]
pub enum AuthState {
    #[default]
    Anonymous,
    Authenticated {
        name: String,
        role: UserRole,
    },
}

impl AuthState {
    pub fn is_authenticated(&self) -> bool {
        matches!(self, AuthState::Authenticated { .. })
    }

    pub fn name(&self) -> &str {
        match self {
            AuthState::Anonymous => "",
            AuthState::Authenticated { name, .. } => name,
        }
    }

    pub fn role(&self) -> Option<UserRole> {
        match self {
            AuthState::Anonymous => None,
            AuthState::Authenticated { role, .. } => Some(*role),
        }
    }

    pub fn has_role(&self, role: UserRole) -> bool {
        self.role() == Some(role)
    }

    /// Convenience wrapper — prefer `has_role(UserRole::SuperAdmin)` in new code.
    pub fn is_admin(&self) -> bool {
        self.has_role(UserRole::SuperAdmin)
    }
}

/// Verifies the session server-side by calling `get_me`. Returns an [`AuthState`]
/// with the name sourced from the API — use this for all access-controlled pages.
pub async fn verify_auth(env: &impl WebsiteEnv, auth: &AuthInfo) -> AuthState {
    let Some(ref token) = auth.token else {
        return AuthState::Anonymous;
    };
    match env.user_api().get_me(token).await {
        Ok(info) => AuthState::Authenticated {
            name: info.name,
            role: info.role,
        },
        Err(_) => AuthState::Anonymous,
    }
}

// ── Per-request env extractor ────────────────────────────────────────────

/// Axum extractor that creates a per-request `WebsiteState` with an isolated
/// token store seeded from the request's cookies. This prevents cross-user
/// token leakage under concurrent SSR traffic.
pub struct RequestEnv(pub WebsiteState);

impl<S> FromRequestParts<S> for RequestEnv
where
    S: Send + Sync,
    WebsiteState: axum::extract::FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut http::request::Parts,
        state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let base_state = WebsiteState::from_ref(state);
            let jar = CookieJar::from_headers(&parts.headers);

            let access = jar.get("mvs_token").map(|c| c.value().to_string());
            let refresh = jar.get("mvs_refresh").map(|c| c.value().to_string());

            let env = match (access, refresh) {
                (Some(token), Some(refresh)) => base_state.with_tokens(token, refresh),
                _ => base_state.with_empty_tokens(),
            };

            Ok(RequestEnv(env))
        }
    }
}

// ── Cookie helpers ───────────────────────────────────────────────────────

/// Builds a hardened cookie with HttpOnly, SameSite=Lax, Secure (in prod),
/// and a default max-age of 24 hours.
pub fn secure_cookie<'a>(name: &'a str, value: String) -> Cookie<'a> {
    let is_prod = std::env::var("COOKIE_SECURE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let mut builder = Cookie::build((name, value))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::hours(24));

    if is_prod {
        builder = builder.secure(true);
    }

    builder.build()
}

/// Builds a non-HttpOnly cookie for values the UI reads (e.g. role, username).
/// Still hardened with SameSite=Lax, Secure, and max-age.
pub fn ui_cookie<'a>(name: &'a str, value: String) -> Cookie<'a> {
    let is_prod = std::env::var("COOKIE_SECURE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let mut builder = Cookie::build((name, value))
        .http_only(false)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::hours(24));

    if is_prod {
        builder = builder.secure(true);
    }

    builder.build()
}

// ── CSRF middleware ──────────────────────────────────────────────────────

/// Validates that state-changing requests (POST, PUT, DELETE, PATCH) originate
/// from the same site by checking the Origin and Referer headers against the
/// Host header.
pub async fn csrf_middleware(request: axum::extract::Request, next: Next) -> Response {
    let method = request.method().clone();

    // Safe methods don't need CSRF validation
    if method == http::Method::GET
        || method == http::Method::HEAD
        || method == http::Method::OPTIONS
    {
        return next.run(request).await;
    }

    let host = request
        .headers()
        .get(http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.to_string());

    let origin = request
        .headers()
        .get(http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|o| extract_host_from_url(o));

    let referer = request
        .headers()
        .get(http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .map(|r| extract_host_from_url(r));

    if let Some(ref host) = host {
        // If Origin is present, it must match Host
        if let Some(ref origin_host) = origin {
            if origin_host != host {
                tracing::warn!(
                    origin = ?origin_host,
                    host = ?host,
                    "CSRF: Origin does not match Host"
                );
                return (StatusCode::FORBIDDEN, "CSRF validation failed").into_response();
            }
            // Origin matched — pass through
            return next.run(request).await;
        }

        // No Origin — fall back to Referer check
        if let Some(ref referer_host) = referer {
            if referer_host != host {
                tracing::warn!(
                    referer = ?referer_host,
                    host = ?host,
                    "CSRF: Referer does not match Host"
                );
                return (StatusCode::FORBIDDEN, "CSRF validation failed").into_response();
            }
            return next.run(request).await;
        }
    }

    // No Origin and no Referer — allow for now (some browsers strip both on
    // same-origin navigation). SameSite=Lax cookies are the primary defence.
    next.run(request).await
}

// ── Token refresh middleware ─────────────────────────────────────────────

/// Axum middleware that transparently refreshes an expired JWT access token on
/// every request. Flow:
///   1. Decode the JWT payload (no signature check — just read `exp`).
///   2. If missing or within 60 s of expiry AND a refresh token cookie is present,
///      call POST /auth/refresh.
///   3. On success: rewrite the Cookie request header so downstream handlers and
///      `verify_auth` see the new token; append Set-Cookie headers to the response.
///   4. On failure (revoked/expired refresh token): zero-out all session cookies so
///      the browser stops sending stale tokens.
///
/// This runs before every handler — no per-handler changes needed.
pub async fn token_refresh_middleware(
    State(state): State<crate::state::WebsiteState>,
    mut request: Request,
    next: Next,
) -> Response {
    let jar = CookieJar::from_headers(request.headers());
    let access_token = jar.get("mvs_token").map(|c| c.value().to_string());
    let refresh_token = jar.get("mvs_refresh").map(|c| c.value().to_string());

    let needs_refresh = match &access_token {
        // No access token but refresh token present — recover the session.
        None => refresh_token.is_some(),
        // Access token present — refresh proactively if it expires within 60 s.
        Some(token) => is_jwt_expired(token),
    };

    if needs_refresh {
        if let Some(refresh) = refresh_token {
            let env = state.with_empty_tokens();

            match env.user_api().refresh(&refresh).await {
                Ok(new_tokens) => {
                    // Inject the new tokens into the request's Cookie header so
                    // every downstream extractor (RequestEnv, verify_auth) sees
                    // the refreshed session without knowing a refresh happened.
                    replace_request_cookie(&mut request, "mvs_token", &new_tokens.access_token);
                    replace_request_cookie(&mut request, "mvs_refresh", &new_tokens.refresh_token);

                    let mut response = next.run(request).await;

                    // Persist the new tokens to the browser.
                    append_set_cookie(&mut response, "mvs_token", &new_tokens.access_token, true);
                    append_set_cookie(
                        &mut response,
                        "mvs_refresh",
                        &new_tokens.refresh_token,
                        true,
                    );

                    tracing::debug!("JWT refreshed transparently");
                    return response;
                }
                Err(e) => {
                    // Refresh token is invalid or revoked — clear all session
                    // cookies so the browser doesn't keep retrying on every page.
                    tracing::info!(error = %e, "Refresh token rejected; clearing session cookies");
                    let mut response = next.run(request).await;
                    for name in ["mvs_token", "mvs_refresh", "mvs_role", "mvs_user_name"] {
                        expire_cookie(&mut response, name);
                    }
                    return response;
                }
            }
        }
    }

    next.run(request).await
}

/// Decodes the JWT payload without verifying the signature and checks the `exp`
/// claim. Returns `true` if the token is malformed, missing an `exp`, or will
/// expire within the next 60 seconds (proactive refresh buffer).
fn is_jwt_expired(token: &str) -> bool {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() != 3 {
        return true;
    }
    // Pad to a multiple of 4 to satisfy the base64 decoder.
    let padded = match parts[1].len() % 4 {
        0 => parts[1].to_string(),
        n => format!("{}{}", parts[1], "=".repeat(4 - n)),
    };
    let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(padded.trim_end_matches('=')) else {
        return true;
    };
    let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&payload_bytes) else {
        return true;
    };
    let Some(exp) = claims.get("exp").and_then(|v| v.as_u64()) else {
        return true;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX);
    // Proactively refresh within the last 60 seconds of the token's lifetime.
    now + 60 >= exp
}

/// Replaces or inserts a single named cookie in the request's `Cookie` header.
/// Other cookies are preserved unchanged.
fn replace_request_cookie(request: &mut Request, name: &str, value: &str) {
    let existing = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let prefix = format!("{}=", name);
    let mut parts: Vec<&str> = existing
        .split("; ")
        .filter(|c| !c.is_empty() && !c.starts_with(&prefix))
        .collect();
    let new_pair = format!("{}={}", name, value);
    parts.push(&new_pair);

    if let Ok(val) = HeaderValue::from_str(&parts.join("; ")) {
        request.headers_mut().insert(header::COOKIE, val);
    }
}

/// Appends a `Set-Cookie` header to the response. `http_only` controls whether
/// the cookie is readable by JavaScript (false for UI-readable cookies).
fn append_set_cookie(response: &mut Response, name: &str, value: &str, http_only: bool) {
    let is_prod = std::env::var("COOKIE_SECURE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let mut header_val = format!("{}={}; Path=/; SameSite=Lax; Max-Age=86400", name, value);
    if http_only {
        header_val.push_str("; HttpOnly");
    }
    if is_prod {
        header_val.push_str("; Secure");
    }
    if let Ok(val) = HeaderValue::from_str(&header_val) {
        response.headers_mut().append(header::SET_COOKIE, val);
    }
}

/// Appends a `Set-Cookie` header that immediately expires the named cookie,
/// effectively deleting it from the browser.
fn expire_cookie(response: &mut Response, name: &str) {
    let header_val = format!("{}=; Path=/; Max-Age=0; SameSite=Lax", name);
    if let Ok(val) = HeaderValue::from_str(&header_val) {
        response.headers_mut().append(header::SET_COOKIE, val);
    }
}

/// Extracts the host:port portion from a URL string.
fn extract_host_from_url(url: &str) -> String {
    // "https://example.com:3000/path" → "example.com:3000"
    url.split("//")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}
