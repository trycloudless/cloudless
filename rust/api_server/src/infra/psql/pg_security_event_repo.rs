use api_types::security_event::{
    ListSecurityEventsResponse, SecurityEventSummary, SecurityEventType, StoreSecurityEventRequest,
    StoreSecurityEventResponse,
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::SecurityEventRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgSecurityEventRepo {
    pub pool: PgPool,
}

impl PgSecurityEventRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SecurityEventRepo for PgSecurityEventRepo {
    async fn store(
        &self,
        user_id: Uuid,
        request: StoreSecurityEventRequest,
    ) -> CoreResult<StoreSecurityEventResponse> {
        let id = Uuid::now_v7();
        let event_type_str = request.event_type.to_string();

        sqlx::query!(
            r#"
            INSERT INTO security_events (id, user_id, event_type, physical_device_id, ip_address, user_agent)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            id,
            user_id,
            event_type_str,
            request.physical_device_id,
            request.ip_address,
            request.user_agent,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(StoreSecurityEventResponse { id })
    }

    async fn list(&self, user_id: Uuid) -> CoreResult<ListSecurityEventsResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT id, event_type, physical_device_id, ip_address, user_agent, created_at
            FROM security_events
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let events = rows
            .into_iter()
            .filter_map(|row| {
                let event_type: SecurityEventType = row.event_type.parse().ok()?;
                Some(SecurityEventSummary {
                    id: row.id,
                    event_type,
                    physical_device_id: row.physical_device_id,
                    ip_address: row.ip_address,
                    user_agent: row.user_agent,
                    created_at: row.created_at,
                })
            })
            .collect();

        Ok(ListSecurityEventsResponse { events })
    }
}
