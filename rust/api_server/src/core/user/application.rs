use crate::core::{
    CoreResult,
    ports::UserRepo,
    subscription::{application as subscription_app, env::SubscriptionEnv},
    user::env::UserEnv,
};
use api_types::user::{AdminUserListResponse, UserCreateRequest, UserCreateResponse, UserInfo};
use uuid::Uuid;

pub async fn create_user<E: UserEnv + SubscriptionEnv>(
    env: E,
    payload: UserCreateRequest,
) -> CoreResult<UserCreateResponse> {
    let user = env.user_repo().create_user(payload).await?;
    subscription_app::ensure_subscription(&env, user.id).await?;
    Ok(user)
}

pub async fn get_me<E: UserEnv>(env: E, user_id: Uuid) -> CoreResult<UserInfo> {
    env.user_repo().find_by_id(user_id).await
}

pub async fn list_all_users<E: UserEnv>(
    env: E,
    page: u32,
    per_page: u32,
) -> CoreResult<AdminUserListResponse> {
    let (users, total_count) = env.user_repo().list_all_paginated(page, per_page).await?;
    Ok(AdminUserListResponse {
        users,
        total_count,
        page,
        per_page,
    })
}
