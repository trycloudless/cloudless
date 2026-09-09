use crate::{app_env::AppEnv, web::routes::health_check};
use axum::{Router, routing::get};
pub fn create_router() -> Router<AppEnv> {
    Router::new().route("/", get(health_check))
}
