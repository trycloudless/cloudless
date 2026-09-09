use crate::web::routes::AuthContext;
use crate::web::web_app_error::WebAppResult;
use crate::{app_env::AppEnv, core::user::application};
use api_types::user::{
    AdminUserListResponse, UserCreateRequest, UserCreateResponse, UserInfo, UserListQueryParams,
};
use axum::Extension;
use axum::extract::{Query, State};
use axum::{
    Json, Router,
    routing::{get, post},
};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/create", post(create_user))
        .route("/me", get(get_me))
        .route("/list", get(list_all_users))
}

async fn create_user(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    Json(payload): Json<UserCreateRequest>,
) -> WebAppResult<Json<UserCreateResponse>> {
    let user = application::create_user(env, payload).await?;
    Ok(Json(user))
}

async fn get_me(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<UserInfo>> {
    let user = application::get_me(env, ctx.user_id).await?;
    Ok(Json(user))
}

async fn list_all_users(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    Query(params): Query<UserListQueryParams>,
) -> WebAppResult<Json<AdminUserListResponse>> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);
    let resp = application::list_all_users(env, page, per_page).await?;
    Ok(Json(resp))
}
