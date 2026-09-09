use crate::{app_env::AppEnv, core::CoreError, web::routes::AuthContext};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

pub async fn jwt_auth(
    State(env): State<AppEnv>,
    mut req: Request,
    next: Next,
) -> Result<Response, CoreError> {
    let auth = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| CoreError::authentication("Missing or invalid Authorization header"))?;

    let claims = env
        .jwt_service
        .verify(auth)
        .map_err(|e| CoreError::Authentication {
            message: "Invalid or expired token".into(),
            source: Some(Box::new(e)),
        })?;

    let auth_context = AuthContext {
        user_id: claims.sub,
    };

    req.extensions_mut().insert(auth_context);

    Ok(next.run(req).await)
}
