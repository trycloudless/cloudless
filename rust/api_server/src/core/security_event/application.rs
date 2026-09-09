use crate::core::{CoreResult, ports::SecurityEventRepo, security_event::env::SecurityEventEnv};
use api_types::security_event::{
    ListSecurityEventsResponse, StoreSecurityEventRequest, StoreSecurityEventResponse,
};
use uuid::Uuid;

/// Store a security audit event for the given user.
pub async fn store<E: SecurityEventEnv>(
    env: E,
    user_id: Uuid,
    request: StoreSecurityEventRequest,
) -> CoreResult<StoreSecurityEventResponse> {
    let response = env.security_event_repo().store(user_id, request).await?;
    Ok(response)
}

/// List all security audit events for the given user, ordered by most recent first.
pub async fn list<E: SecurityEventEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<ListSecurityEventsResponse> {
    let response = env.security_event_repo().list(user_id).await?;
    Ok(response)
}
