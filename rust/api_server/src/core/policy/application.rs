use crate::core::{CoreResult, policy::env::PolicyEnv, ports::PolicyRepo};
use api_types::policy::*;
use uuid::Uuid;

pub async fn create_policy<E: PolicyEnv>(
    env: &E,
    req: CreatePolicyRequest,
) -> CoreResult<CreatePolicyResponse> {
    env.policy_repo().create_policy(req).await
}

pub async fn update_policy<E: PolicyEnv>(
    env: &E,
    req: UpdatePolicyRequest,
) -> CoreResult<PolicyResponse> {
    env.policy_repo().update_policy(req).await
}

pub async fn list_policies<E: PolicyEnv>(env: &E) -> CoreResult<ListPoliciesResponse> {
    env.policy_repo().list_policies().await
}

pub async fn create_version<E: PolicyEnv>(
    env: &E,
    req: CreatePolicyVersionRequest,
) -> CoreResult<CreatePolicyVersionResponse> {
    env.policy_repo().create_version(req).await
}

pub async fn update_version<E: PolicyEnv>(
    env: &E,
    req: UpdatePolicyVersionRequest,
) -> CoreResult<PolicyVersionResponse> {
    // Business rule: only draft versions can be updated (enforced in repo)
    env.policy_repo().update_version(req).await
}

pub async fn publish_version<E: PolicyEnv>(
    env: &E,
    req: PublishPolicyVersionRequest,
) -> CoreResult<PolicyVersionResponse> {
    // Business rule: only draft versions can be published (enforced in repo)
    env.policy_repo().publish_version(req).await
}

pub async fn list_versions<E: PolicyEnv>(
    env: &E,
    policy_id: Uuid,
) -> CoreResult<ListPolicyVersionsResponse> {
    env.policy_repo().list_versions(policy_id).await
}

pub async fn get_version<E: PolicyEnv>(env: &E, id: Uuid) -> CoreResult<PolicyVersionResponse> {
    env.policy_repo().get_version(id).await
}

pub async fn get_pending_policies<E: PolicyEnv>(
    env: &E,
    user_id: Uuid,
) -> CoreResult<GetPendingPoliciesResponse> {
    env.policy_repo().get_pending_policies(user_id).await
}

pub async fn accept_policy<E: PolicyEnv>(
    env: &E,
    user_id: Uuid,
    req: AcceptPolicyRequest,
) -> CoreResult<AcceptPolicyResponse> {
    env.policy_repo().accept_policy(user_id, req).await
}

pub async fn skip_policy<E: PolicyEnv>(
    env: &E,
    user_id: Uuid,
    req: SkipPolicyRequest,
) -> CoreResult<SkipPolicyResponse> {
    // Business rule: reject if times_skipped >= max_skips (enforced in repo)
    env.policy_repo().skip_policy(user_id, req).await
}

pub async fn get_checkout_policy<E: PolicyEnv>(
    env: &E,
    user_id: Uuid,
) -> CoreResult<GetCheckoutPolicyResponse> {
    env.policy_repo().get_checkout_policy(user_id).await
}

pub async fn get_public_policy<E: PolicyEnv>(
    env: &E,
    policy_type: api_types::policy::PolicyType,
) -> CoreResult<Option<api_types::policy::PublicPolicyResponse>> {
    env.policy_repo().get_latest_published(policy_type).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreError;
    use async_trait::async_trait;
    use chrono::Utc;

    // ── Mock PolicyRepo ────────────────────────────────────────────

    /// Controls which operations should fail in the mock.
    #[derive(Default)]
    struct MockFailures {
        create_policy: bool,
        update_policy: bool,
        list_policies: bool,
        create_version: bool,
        update_version: bool,
        publish_version: bool,
        list_versions: bool,
        get_version: bool,
        get_pending: bool,
        accept: bool,
        skip: bool,
        get_latest_published: bool,
        get_checkout_policy: bool,
        /// When true the mock returns `policy: None` (simulates already-accepted).
        get_checkout_policy_returns_accepted: bool,
    }

    struct MockPolicyRepo {
        failures: MockFailures,
    }

    impl MockPolicyRepo {
        fn new() -> Self {
            Self {
                failures: MockFailures::default(),
            }
        }

        fn failing(failures: MockFailures) -> Self {
            Self { failures }
        }
    }

    fn mock_error() -> CoreError {
        CoreError::internal("mock error")
    }

    #[async_trait]
    impl PolicyRepo for MockPolicyRepo {
        async fn create_policy(
            &self,
            _req: CreatePolicyRequest,
        ) -> CoreResult<CreatePolicyResponse> {
            if self.failures.create_policy {
                return Err(mock_error());
            }
            Ok(CreatePolicyResponse { id: Uuid::now_v7() })
        }

        async fn update_policy(&self, req: UpdatePolicyRequest) -> CoreResult<PolicyResponse> {
            if self.failures.update_policy {
                return Err(mock_error());
            }
            Ok(PolicyResponse {
                id: req.id,
                policy_type: PolicyType::PrivacyPolicy,
                title: req.title.unwrap_or_else(|| "Privacy Policy".into()),
                description: req.description.unwrap_or_default(),
                is_required: req.is_required.unwrap_or(true),
                max_skips: req.max_skips.unwrap_or(0),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
        }

        async fn list_policies(&self) -> CoreResult<ListPoliciesResponse> {
            if self.failures.list_policies {
                return Err(mock_error());
            }
            Ok(ListPoliciesResponse {
                list: vec![PolicyResponse {
                    id: Uuid::now_v7(),
                    policy_type: PolicyType::PrivacyPolicy,
                    title: "Privacy Policy".into(),
                    description: "Our privacy policy".into(),
                    is_required: true,
                    max_skips: 0,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                }],
            })
        }

        async fn create_version(
            &self,
            _req: CreatePolicyVersionRequest,
        ) -> CoreResult<CreatePolicyVersionResponse> {
            if self.failures.create_version {
                return Err(mock_error());
            }
            Ok(CreatePolicyVersionResponse {
                id: Uuid::now_v7(),
                version: 1,
            })
        }

        async fn update_version(
            &self,
            req: UpdatePolicyVersionRequest,
        ) -> CoreResult<PolicyVersionResponse> {
            if self.failures.update_version {
                return Err(mock_error());
            }
            Ok(PolicyVersionResponse {
                id: req.id,
                policy_id: Uuid::now_v7(),
                version: 1,
                content: req.content,
                status: "draft".into(),
                published_at: None,
                created_at: Utc::now(),
            })
        }

        async fn publish_version(
            &self,
            req: PublishPolicyVersionRequest,
        ) -> CoreResult<PolicyVersionResponse> {
            if self.failures.publish_version {
                return Err(mock_error());
            }
            Ok(PolicyVersionResponse {
                id: req.id,
                policy_id: Uuid::now_v7(),
                version: 1,
                content: "# Privacy Policy\nContent here.".into(),
                status: "published".into(),
                published_at: Some(Utc::now()),
                created_at: Utc::now(),
            })
        }

        async fn list_versions(&self, policy_id: Uuid) -> CoreResult<ListPolicyVersionsResponse> {
            if self.failures.list_versions {
                return Err(mock_error());
            }
            Ok(ListPolicyVersionsResponse {
                list: vec![PolicyVersionSummary {
                    id: Uuid::now_v7(),
                    policy_id,
                    version: 1,
                    status: "draft".into(),
                    published_at: None,
                    created_at: Utc::now(),
                }],
            })
        }

        async fn get_version(&self, id: Uuid) -> CoreResult<PolicyVersionResponse> {
            if self.failures.get_version {
                return Err(mock_error());
            }
            Ok(PolicyVersionResponse {
                id,
                policy_id: Uuid::now_v7(),
                version: 1,
                content: "# Policy Content".into(),
                status: "draft".into(),
                published_at: None,
                created_at: Utc::now(),
            })
        }

        async fn get_pending_policies(
            &self,
            _user_id: Uuid,
        ) -> CoreResult<GetPendingPoliciesResponse> {
            if self.failures.get_pending {
                return Err(mock_error());
            }
            Ok(GetPendingPoliciesResponse {
                list: vec![PendingPolicyResponse {
                    policy_version_id: Uuid::now_v7(),
                    policy_type: PolicyType::TermsOfService,
                    policy_title: "Terms of Service".into(),
                    version: 1,
                    content: "# Terms\nYou agree to these terms.".into(),
                    max_skips: 3,
                    times_skipped: 1,
                }],
            })
        }

        async fn accept_policy(
            &self,
            _user_id: Uuid,
            _req: AcceptPolicyRequest,
        ) -> CoreResult<AcceptPolicyResponse> {
            if self.failures.accept {
                return Err(mock_error());
            }
            Ok(AcceptPolicyResponse { id: Uuid::now_v7() })
        }

        async fn skip_policy(
            &self,
            _user_id: Uuid,
            _req: SkipPolicyRequest,
        ) -> CoreResult<SkipPolicyResponse> {
            if self.failures.skip {
                return Err(mock_error());
            }
            Ok(SkipPolicyResponse {
                id: Uuid::now_v7(),
                remaining_skips: 2,
            })
        }

        async fn get_latest_published(
            &self,
            policy_type: PolicyType,
        ) -> CoreResult<Option<PublicPolicyResponse>> {
            if self.failures.get_latest_published {
                return Err(mock_error());
            }
            Ok(Some(PublicPolicyResponse {
                policy_type,
                title: "Terms of Service".into(),
                version: 1,
                content: "# Terms of Service\nContent here.".into(),
                published_at: Utc::now(),
            }))
        }

        async fn get_checkout_policy(
            &self,
            _user_id: Uuid,
        ) -> CoreResult<GetCheckoutPolicyResponse> {
            if self.failures.get_checkout_policy {
                return Err(mock_error());
            }
            if self.failures.get_checkout_policy_returns_accepted {
                return Ok(GetCheckoutPolicyResponse { policy: None });
            }
            Ok(GetCheckoutPolicyResponse {
                policy: Some(PendingPolicyResponse {
                    policy_version_id: Uuid::now_v7(),
                    policy_type: PolicyType::RefundAndCancellation,
                    policy_title: "Refund and Cancellation".into(),
                    version: 1,
                    content: "# Refund Policy\nContent here.".into(),
                    max_skips: 0,
                    times_skipped: 0,
                }),
            })
        }
    }

    // ── Mock PolicyEnv ─────────────────────────────────────────────

    struct MockPolicyEnv {
        repo: MockPolicyRepo,
    }

    impl MockPolicyEnv {
        fn new() -> Self {
            Self {
                repo: MockPolicyRepo::new(),
            }
        }

        fn with_failures(failures: MockFailures) -> Self {
            Self {
                repo: MockPolicyRepo::failing(failures),
            }
        }
    }

    impl PolicyEnv for MockPolicyEnv {
        type Repo = MockPolicyRepo;
        fn policy_repo(&self) -> &Self::Repo {
            &self.repo
        }
    }

    // ── Tests: create_policy ───────────────────────────────────────

    #[tokio::test]
    async fn test_create_policy_success() {
        let env = MockPolicyEnv::new();
        let req = CreatePolicyRequest {
            policy_type: PolicyType::PrivacyPolicy,
            title: "Privacy Policy".into(),
            description: "Our privacy policy".into(),
            is_required: true,
            max_skips: 0,
        };
        let result = create_policy(&env, req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_policy_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            create_policy: true,
            ..Default::default()
        });
        let req = CreatePolicyRequest {
            policy_type: PolicyType::PrivacyPolicy,
            title: "Privacy Policy".into(),
            description: "".into(),
            is_required: true,
            max_skips: 0,
        };
        let result = create_policy(&env, req).await;
        assert!(result.is_err());
    }

    // ── Tests: update_policy ───────────────────────────────────────

    #[tokio::test]
    async fn test_update_policy_success() {
        let env = MockPolicyEnv::new();
        let req = UpdatePolicyRequest {
            id: Uuid::now_v7(),
            title: Some("Updated Title".into()),
            description: None,
            is_required: None,
            max_skips: Some(5),
        };
        let result = update_policy(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.title, "Updated Title");
        assert_eq!(resp.max_skips, 5);
    }

    #[tokio::test]
    async fn test_update_policy_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            update_policy: true,
            ..Default::default()
        });
        let req = UpdatePolicyRequest {
            id: Uuid::now_v7(),
            title: Some("Updated".into()),
            description: None,
            is_required: None,
            max_skips: None,
        };
        let result = update_policy(&env, req).await;
        assert!(result.is_err());
    }

    // ── Tests: list_policies ───────────────────────────────────────

    #[tokio::test]
    async fn test_list_policies_success() {
        let env = MockPolicyEnv::new();
        let result = list_policies(&env).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.list.len(), 1);
        assert_eq!(resp.list[0].policy_type, PolicyType::PrivacyPolicy);
    }

    #[tokio::test]
    async fn test_list_policies_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            list_policies: true,
            ..Default::default()
        });
        let result = list_policies(&env).await;
        assert!(result.is_err());
    }

    // ── Tests: create_version ──────────────────────────────────────

    #[tokio::test]
    async fn test_create_version_success() {
        let env = MockPolicyEnv::new();
        let req = CreatePolicyVersionRequest {
            policy_id: Uuid::now_v7(),
            content: "# Privacy Policy\nVersion 1 content.".into(),
        };
        let result = create_version(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.version, 1);
    }

    #[tokio::test]
    async fn test_create_version_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            create_version: true,
            ..Default::default()
        });
        let req = CreatePolicyVersionRequest {
            policy_id: Uuid::now_v7(),
            content: "content".into(),
        };
        let result = create_version(&env, req).await;
        assert!(result.is_err());
    }

    // ── Tests: update_version ──────────────────────────────────────

    #[tokio::test]
    async fn test_update_version_success() {
        let env = MockPolicyEnv::new();
        let version_id = Uuid::now_v7();
        let req = UpdatePolicyVersionRequest {
            id: version_id,
            content: "Updated draft content".into(),
        };
        let result = update_version(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, version_id);
        assert_eq!(resp.content, "Updated draft content");
        assert_eq!(resp.status, "draft");
    }

    #[tokio::test]
    async fn test_update_version_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            update_version: true,
            ..Default::default()
        });
        let req = UpdatePolicyVersionRequest {
            id: Uuid::now_v7(),
            content: "content".into(),
        };
        let result = update_version(&env, req).await;
        assert!(result.is_err());
    }

    // ── Tests: publish_version ─────────────────────────────────────

    #[tokio::test]
    async fn test_publish_version_success() {
        let env = MockPolicyEnv::new();
        let version_id = Uuid::now_v7();
        let req = PublishPolicyVersionRequest { id: version_id };
        let result = publish_version(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, version_id);
        assert_eq!(resp.status, "published");
        assert!(resp.published_at.is_some());
    }

    #[tokio::test]
    async fn test_publish_version_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            publish_version: true,
            ..Default::default()
        });
        let req = PublishPolicyVersionRequest { id: Uuid::now_v7() };
        let result = publish_version(&env, req).await;
        assert!(result.is_err());
    }

    // ── Tests: list_versions ───────────────────────────────────────

    #[tokio::test]
    async fn test_list_versions_success() {
        let env = MockPolicyEnv::new();
        let policy_id = Uuid::now_v7();
        let result = list_versions(&env, policy_id).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.list.len(), 1);
        assert_eq!(resp.list[0].policy_id, policy_id);
    }

    #[tokio::test]
    async fn test_list_versions_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            list_versions: true,
            ..Default::default()
        });
        let result = list_versions(&env, Uuid::now_v7()).await;
        assert!(result.is_err());
    }

    // ── Tests: get_version ─────────────────────────────────────────

    #[tokio::test]
    async fn test_get_version_success() {
        let env = MockPolicyEnv::new();
        let version_id = Uuid::now_v7();
        let result = get_version(&env, version_id).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, version_id);
    }

    #[tokio::test]
    async fn test_get_version_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            get_version: true,
            ..Default::default()
        });
        let result = get_version(&env, Uuid::now_v7()).await;
        assert!(result.is_err());
    }

    // ── Tests: get_pending_policies ────────────────────────────────

    #[tokio::test]
    async fn test_get_pending_policies_success() {
        let env = MockPolicyEnv::new();
        let user_id = Uuid::now_v7();
        let result = get_pending_policies(&env, user_id).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.list.len(), 1);
        assert_eq!(resp.list[0].policy_type, PolicyType::TermsOfService);
        assert_eq!(resp.list[0].times_skipped, 1);
        assert_eq!(resp.list[0].max_skips, 3);
    }

    #[tokio::test]
    async fn test_get_pending_policies_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            get_pending: true,
            ..Default::default()
        });
        let result = get_pending_policies(&env, Uuid::now_v7()).await;
        assert!(result.is_err());
    }

    // ── Tests: accept_policy ───────────────────────────────────────

    #[tokio::test]
    async fn test_accept_policy_success() {
        let env = MockPolicyEnv::new();
        let user_id = Uuid::now_v7();
        let req = AcceptPolicyRequest {
            policy_version_id: Uuid::now_v7(),
            ip_address: Some("192.168.1.1".into()),
            user_agent: Some("TestAgent/1.0".into()),
            physical_device_id: Some("device-abc-123".into()),
        };
        let result = accept_policy(&env, user_id, req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_accept_policy_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            accept: true,
            ..Default::default()
        });
        let req = AcceptPolicyRequest {
            policy_version_id: Uuid::now_v7(),
            ip_address: None,
            user_agent: None,
            physical_device_id: None,
        };
        let result = accept_policy(&env, Uuid::now_v7(), req).await;
        assert!(result.is_err());
    }

    // ── Tests: skip_policy ─────────────────────────────────────────

    #[tokio::test]
    async fn test_skip_policy_success() {
        let env = MockPolicyEnv::new();
        let user_id = Uuid::now_v7();
        let req = SkipPolicyRequest {
            policy_version_id: Uuid::now_v7(),
            ip_address: Some("10.0.0.1".into()),
            user_agent: Some("TestAgent/1.0".into()),
            physical_device_id: Some("device-xyz-789".into()),
        };
        let result = skip_policy(&env, user_id, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.remaining_skips, 2);
    }

    #[tokio::test]
    async fn test_skip_policy_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            skip: true,
            ..Default::default()
        });
        let req = SkipPolicyRequest {
            policy_version_id: Uuid::now_v7(),
            ip_address: None,
            user_agent: None,
            physical_device_id: None,
        };
        let result = skip_policy(&env, Uuid::now_v7(), req).await;
        assert!(result.is_err());
    }

    // ── Tests: get_checkout_policy ─────────────────────────────────

    #[tokio::test]
    async fn test_get_checkout_policy_success() {
        let env = MockPolicyEnv::new();
        let result = get_checkout_policy(&env, Uuid::now_v7()).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        let policy = resp.policy.expect("pending policy should be present");
        assert_eq!(policy.policy_type, PolicyType::RefundAndCancellation);
        assert_eq!(policy.max_skips, 0);
        assert_eq!(policy.times_skipped, 0);
    }

    #[tokio::test]
    async fn test_get_checkout_policy_failure() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            get_checkout_policy: true,
            ..Default::default()
        });
        let result = get_checkout_policy(&env, Uuid::now_v7()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_checkout_policy_already_accepted() {
        let env = MockPolicyEnv::with_failures(MockFailures {
            get_checkout_policy_returns_accepted: true,
            ..Default::default()
        });
        let result = get_checkout_policy(&env, Uuid::now_v7()).await;
        assert!(result.is_ok());
        assert!(
            result.unwrap().policy.is_none(),
            "should be None when already accepted"
        );
    }
}
