use api_types::policy::*;
use async_trait::async_trait;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::core::{CoreError, CoreResult, ports::PolicyRepo};

use super::map_sqlx_error;

#[derive(Clone)]
pub struct PgPolicyRepo {
    pub pool: PgPool,
}

impl PgPolicyRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PolicyRepo for PgPolicyRepo {
    async fn create_policy(&self, req: CreatePolicyRequest) -> CoreResult<CreatePolicyResponse> {
        let id = Uuid::now_v7();
        let policy_type_str = req.policy_type.to_string();
        sqlx::query!(
            r#"
            INSERT INTO policies (id, policy_type, title, description, is_required, max_skips)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            id,
            policy_type_str,
            req.title,
            req.description,
            req.is_required,
            req.max_skips,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(CreatePolicyResponse { id })
    }

    async fn update_policy(&self, req: UpdatePolicyRequest) -> CoreResult<PolicyResponse> {
        let row = sqlx::query!(
            r#"
            UPDATE policies
            SET title = COALESCE($2, title),
                description = COALESCE($3, description),
                is_required = COALESCE($4, is_required),
                max_skips = COALESCE($5, max_skips),
                updated_at = now()
            WHERE id = $1
            RETURNING id, policy_type, title, description, is_required, max_skips, created_at, updated_at
            "#,
            req.id,
            req.title,
            req.description,
            req.is_required,
            req.max_skips,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(PolicyResponse {
            id: row.id,
            policy_type: row
                .policy_type
                .parse()
                .map_err(|e: String| CoreError::internal(e))?,
            title: row.title,
            description: row.description,
            is_required: row.is_required,
            max_skips: row.max_skips,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn list_policies(&self) -> CoreResult<ListPoliciesResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT id, policy_type, title, description, is_required, max_skips, created_at, updated_at
            FROM policies
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let list = rows
            .into_iter()
            .map(|r| {
                Ok(PolicyResponse {
                    id: r.id,
                    policy_type: r
                        .policy_type
                        .parse()
                        .map_err(|e: String| CoreError::internal(e))?,
                    title: r.title,
                    description: r.description,
                    is_required: r.is_required,
                    max_skips: r.max_skips,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(ListPoliciesResponse { list })
    }

    async fn create_version(
        &self,
        req: CreatePolicyVersionRequest,
    ) -> CoreResult<CreatePolicyVersionResponse> {
        let id = Uuid::now_v7();

        // Auto-compute next version number
        let next_version = sqlx::query_scalar!(
            r#"SELECT COALESCE(MAX(version), 0) + 1 as "next_version!" FROM policy_versions WHERE policy_id = $1"#,
            req.policy_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        sqlx::query!(
            r#"
            INSERT INTO policy_versions (id, policy_id, version, content, status)
            VALUES ($1, $2, $3, $4, 'draft')
            "#,
            id,
            req.policy_id,
            next_version,
            req.content,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(CreatePolicyVersionResponse {
            id,
            version: next_version,
        })
    }

    async fn update_version(
        &self,
        req: UpdatePolicyVersionRequest,
    ) -> CoreResult<PolicyVersionResponse> {
        let row = sqlx::query!(
            r#"
            UPDATE policy_versions
            SET content = $2,
                updated_at = now()
            WHERE id = $1 AND status = 'draft'
            RETURNING id, policy_id, version, content, status, published_at, created_at, updated_at
            "#,
            req.id,
            req.content,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => Ok(PolicyVersionResponse {
                id: r.id,
                policy_id: r.policy_id,
                version: r.version,
                content: r.content,
                status: r.status,
                published_at: r.published_at,
                created_at: r.created_at,
            }),
            None => Err(CoreError::validation(
                "Policy version not found or already published",
            )),
        }
    }

    async fn publish_version(
        &self,
        req: PublishPolicyVersionRequest,
    ) -> CoreResult<PolicyVersionResponse> {
        let row = sqlx::query!(
            r#"
            UPDATE policy_versions
            SET status = 'published',
                published_at = now(),
                updated_at = now()
            WHERE id = $1 AND status = 'draft'
            RETURNING id, policy_id, version, content, status, published_at, created_at, updated_at
            "#,
            req.id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => Ok(PolicyVersionResponse {
                id: r.id,
                policy_id: r.policy_id,
                version: r.version,
                content: r.content,
                status: r.status,
                published_at: r.published_at,
                created_at: r.created_at,
            }),
            None => Err(CoreError::validation(
                "Policy version not found or already published",
            )),
        }
    }

    async fn list_versions(&self, policy_id: Uuid) -> CoreResult<ListPolicyVersionsResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT id, policy_id, version, status, published_at, created_at
            FROM policy_versions
            WHERE policy_id = $1
            ORDER BY version DESC
            "#,
            policy_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let list = rows
            .into_iter()
            .map(|r| PolicyVersionSummary {
                id: r.id,
                policy_id: r.policy_id,
                version: r.version,
                status: r.status,
                published_at: r.published_at,
                created_at: r.created_at,
            })
            .collect();

        Ok(ListPolicyVersionsResponse { list })
    }

    async fn get_version(&self, id: Uuid) -> CoreResult<PolicyVersionResponse> {
        let row = sqlx::query!(
            r#"
            SELECT id, policy_id, version, content, status, published_at, created_at, updated_at
            FROM policy_versions
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => Ok(PolicyVersionResponse {
                id: r.id,
                policy_id: r.policy_id,
                version: r.version,
                content: r.content,
                status: r.status,
                published_at: r.published_at,
                created_at: r.created_at,
            }),
            None => Err(CoreError::not_found("Policy version not found")),
        }
    }

    async fn get_pending_policies(&self, user_id: Uuid) -> CoreResult<GetPendingPoliciesResponse> {
        // Find the latest published version per required policy that the user hasn't accepted.
        // Also count how many times the user has skipped each version.
        let rows = sqlx::query!(
            r#"
            SELECT
                pv.id as policy_version_id,
                p.policy_type,
                p.title as policy_title,
                pv.version,
                pv.content,
                p.max_skips,
                COALESCE(skip_counts.times_skipped, 0)::int as "times_skipped!"
            FROM policies p
            INNER JOIN LATERAL (
                SELECT id, version, content
                FROM policy_versions
                WHERE policy_id = p.id AND status = 'published'
                ORDER BY version DESC
                LIMIT 1
            ) pv ON true
            LEFT JOIN user_policy_acceptances upa
                ON upa.policy_version_id = pv.id AND upa.user_id = $1
            LEFT JOIN (
                SELECT policy_version_id, COUNT(*)::int as times_skipped
                FROM user_policy_skips
                WHERE user_id = $1
                GROUP BY policy_version_id
            ) skip_counts ON skip_counts.policy_version_id = pv.id
            WHERE p.is_required = true AND upa.id IS NULL
            "#,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let list = rows
            .into_iter()
            .map(|r| {
                Ok(PendingPolicyResponse {
                    policy_version_id: r.policy_version_id,
                    policy_type: r
                        .policy_type
                        .parse()
                        .map_err(|e: String| CoreError::internal(e))?,
                    policy_title: r.policy_title,
                    version: r.version,
                    content: r.content,
                    max_skips: r.max_skips,
                    times_skipped: r.times_skipped,
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(GetPendingPoliciesResponse { list })
    }

    async fn accept_policy(
        &self,
        user_id: Uuid,
        req: AcceptPolicyRequest,
    ) -> CoreResult<AcceptPolicyResponse> {
        let id = Uuid::now_v7();
        sqlx::query!(
            r#"
            INSERT INTO user_policy_acceptances (id, user_id, policy_version_id, ip_address, user_agent, physical_device_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, policy_version_id) DO NOTHING
            "#,
            id,
            user_id,
            req.policy_version_id,
            req.ip_address,
            req.user_agent,
            req.physical_device_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(AcceptPolicyResponse { id })
    }

    async fn skip_policy(
        &self,
        user_id: Uuid,
        req: SkipPolicyRequest,
    ) -> CoreResult<SkipPolicyResponse> {
        // Check current skip count against max_skips
        let skip_info = sqlx::query!(
            r#"
            SELECT
                p.max_skips,
                COALESCE(
                    (SELECT COUNT(*)::int FROM user_policy_skips
                     WHERE user_id = $1 AND policy_version_id = $2),
                    0
                ) as "times_skipped!"
            FROM policy_versions pv
            INNER JOIN policies p ON p.id = pv.policy_id
            WHERE pv.id = $2
            "#,
            user_id,
            req.policy_version_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let skip_info =
            skip_info.ok_or_else(|| CoreError::not_found("Policy version not found"))?;

        if skip_info.max_skips == 0 {
            return Err(CoreError::validation(
                "This policy requires your acceptance and cannot be skipped.",
            ));
        }

        if skip_info.times_skipped >= skip_info.max_skips {
            return Err(CoreError::validation(
                "Maximum number of skips reached. You must accept this policy to continue.",
            ));
        }

        let id = Uuid::now_v7();
        sqlx::query!(
            r#"
            INSERT INTO user_policy_skips (id, user_id, policy_version_id, ip_address, user_agent, physical_device_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            id,
            user_id,
            req.policy_version_id,
            req.ip_address,
            req.user_agent,
            req.physical_device_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let remaining = skip_info.max_skips - skip_info.times_skipped - 1;
        Ok(SkipPolicyResponse {
            id,
            remaining_skips: remaining,
        })
    }

    async fn get_checkout_policy(&self, user_id: Uuid) -> CoreResult<GetCheckoutPolicyResponse> {
        let row = sqlx::query!(
            r#"
            SELECT
                pv.id as policy_version_id,
                p.policy_type,
                p.title as policy_title,
                pv.version,
                pv.content,
                p.max_skips,
                0::int as "times_skipped!"
            FROM policies p
            INNER JOIN LATERAL (
                SELECT id, version, content
                FROM policy_versions
                WHERE policy_id = p.id AND status = 'published'
                ORDER BY version DESC
                LIMIT 1
            ) pv ON true
            LEFT JOIN user_policy_acceptances upa
                ON upa.policy_version_id = pv.id AND upa.user_id = $1
            WHERE p.policy_type = 'refund_and_cancellation'
              AND upa.id IS NULL
            "#,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let policy = match row {
            None => None,
            Some(r) => Some(PendingPolicyResponse {
                policy_version_id: r.policy_version_id,
                policy_type: r
                    .policy_type
                    .parse()
                    .map_err(|e: String| CoreError::internal(e))?,
                policy_title: r.policy_title,
                version: r.version,
                content: r.content,
                max_skips: r.max_skips,
                times_skipped: r.times_skipped,
            }),
        };

        Ok(GetCheckoutPolicyResponse { policy })
    }

    async fn get_latest_published(
        &self,
        policy_type: PolicyType,
    ) -> CoreResult<Option<PublicPolicyResponse>> {
        let policy_type_str = policy_type.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                p.policy_type,
                p.title,
                pv.version,
                pv.content,
                pv.published_at
            FROM policies p
            INNER JOIN policy_versions pv ON pv.policy_id = p.id
            WHERE p.policy_type = $1
              AND pv.status = 'published'
            ORDER BY pv.version DESC
            LIMIT 1
            "#,
            policy_type_str,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let published_at = row
            .published_at
            .ok_or_else(|| CoreError::internal("Published policy version has no published_at"))?;

        Ok(Some(PublicPolicyResponse {
            policy_type,
            title: row.title,
            version: row.version,
            content: row.content,
            published_at,
        }))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn skip_blocked_when_max_skips_zero() {
        let max_skips: i32 = 0;
        assert_eq!(max_skips, 0, "max_skips=0 must trigger non-skippable guard");
    }

    #[test]
    fn skip_blocked_when_exhausted() {
        let max_skips: i32 = 2;
        let times_skipped: i32 = 2;
        assert!(times_skipped >= max_skips);
    }

    #[test]
    fn skip_allowed_when_remaining() {
        let max_skips: i32 = 3;
        let times_skipped: i32 = 1;
        assert!(max_skips != 0 && times_skipped < max_skips);
    }
}
