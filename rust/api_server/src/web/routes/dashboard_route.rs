use crate::{
    app_env::AppEnv,
    core::dashboard::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::dashboard::GetDashboardStatsResponse;
use axum::{Extension, Json, Router, extract::State, routing::get};

pub fn create_router() -> Router<AppEnv> {
    Router::new().route("/stats", get(get_stats))
}

async fn get_stats(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<GetDashboardStatsResponse>> {
    let response = application::get_stats(env, ctx.user_id).await?;
    Ok(Json(response))
}
