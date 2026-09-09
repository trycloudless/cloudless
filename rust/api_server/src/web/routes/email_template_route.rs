use crate::{
    app_env::AppEnv,
    core::email_template::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::email_template::*;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::get,
};

pub fn protected_router() -> Router<AppEnv> {
    Router::new()
        .route("/", get(list_handler).post(create_handler))
        .route(
            "/{id}",
            get(get_handler).put(update_handler).delete(delete_handler),
        )
}

async fn list_handler(State(env): State<AppEnv>) -> WebAppResult<Json<EmailTemplateListResponse>> {
    let templates = application::list_all(&env).await?;
    Ok(Json(EmailTemplateListResponse { templates }))
}

async fn get_handler(
    State(env): State<AppEnv>,
    Path(id): Path<uuid::Uuid>,
) -> WebAppResult<Json<EmailTemplateResponse>> {
    let template = application::get_by_id(&env, id).await?;
    Ok(Json(template))
}

async fn create_handler(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    Json(req): Json<CreateEmailTemplateRequest>,
) -> WebAppResult<Json<EmailTemplateResponse>> {
    let template = application::create_template(&env, req).await?;
    Ok(Json(template))
}

async fn update_handler(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    Path(id): Path<uuid::Uuid>,
    Json(mut req): Json<UpdateEmailTemplateRequest>,
) -> WebAppResult<Json<EmailTemplateResponse>> {
    req.id = id;
    let template = application::update_template(&env, req).await?;
    Ok(Json(template))
}

async fn delete_handler(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    Path(id): Path<uuid::Uuid>,
) -> WebAppResult<Json<DeleteEmailTemplateResponse>> {
    application::delete_template(&env, id).await?;
    Ok(Json(DeleteEmailTemplateResponse { success: true }))
}
