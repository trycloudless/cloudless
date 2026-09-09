use api_types::{
    auth::{LoginRequest, LoginResponse},
    encrypted_dek::{DekKeyType, GetEncryptedDekRequest, StoreEncryptedDekRequest},
    user::{UserCreateRequest, UserCreateResponse},
};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use tracing::{debug, info};

use crate::{
    applications::user::env::UserEnv,
    domain::{
        dek::Dek,
        kek::Kek,
        recovery_key::{RecoveryKey, RecoverySecret},
    },
    model::base::{AppResult, EncryptedData},
    ports::api::{encrypted_dek_api_port::EncryptedDekApiPort, user_api_port::UserApiPort},
};

pub async fn create_user<E: UserEnv>(
    env: &E,
    request: UserCreateRequest,
) -> AppResult<UserCreateResponse> {
    let user_api = env.user_api();
    let response = user_api.create_user(request).await?;
    info!(user_id = %response.id, email = %response.email, "User created");
    Ok(response)
}

pub async fn login<E: UserEnv>(env: &E, request: LoginRequest) -> AppResult<LoginResponse> {
    let user_api = env.user_api();
    let response = user_api.login(request).await?;
    info!(user_id = %response.user_id, email = %response.email, "User logged in");
    Ok(response)
}

pub async fn update_dek<E: UserEnv>(
    env: &E,
    dek: Dek,
    password: &str,
    key_type: DekKeyType,
) -> AppResult<()> {
    let password_salt = SaltString::generate(&mut OsRng);
    let password_kek = Kek::derive(password, &password_salt)?;
    let password_encrypted_dek = dek.encrypt_dek(&password_kek)?;

    env.encrypted_dek_api()
        .store(StoreEncryptedDekRequest {
            key_type: key_type.clone(),
            encrypted_key: password_encrypted_dek.into(),
            salt: password_salt.to_string(),
        })
        .await?;

    info!(key_type = ?key_type, "DEK encrypted and stored");
    Ok(())
}

pub async fn unlock_dek<E: UserEnv>(
    env: &E,
    password: &str,
    key_type: DekKeyType,
) -> AppResult<Dek> {
    debug!(key_type = %key_type, "Fetching encrypted DEK");
    let dek_response = env
        .encrypted_dek_api()
        .get(GetEncryptedDekRequest {
            key_type: key_type.clone(),
        })
        .await?;

    let salt = SaltString::from_b64(&dek_response.salt)?;
    let kek = Kek::derive(password, &salt)?;
    let encrypted_data: EncryptedData = dek_response.encrypted_key.try_into()?;
    let dek = Dek::decrypt_dek(&kek, &encrypted_data)?;
    info!(key_type = %key_type, "DEK unlocked");
    Ok(dek)
}

/// Generate a recovery key, encrypt the DEK with it, and store on the server.
///
/// Returns the recovery secret as a display string (CLRK-XXXX-...) for the user to save.
/// The salt field is set to "recovery" as a sentinel since HKDF does not use a salt.
pub async fn generate_recovery_key<E: UserEnv>(env: &E, dek: &Dek) -> AppResult<String> {
    let rs = RecoverySecret::generate()?;
    let rk = RecoveryKey::derive(&rs)?;
    let encrypted = rk.encrypt_dek(dek)?;

    env.encrypted_dek_api()
        .store(StoreEncryptedDekRequest {
            key_type: DekKeyType::Recovery,
            encrypted_key: encrypted.into(),
            salt: "recovery".to_string(),
        })
        .await?;

    let display_string = rs.to_display_string();
    info!("Recovery key generated and stored");
    Ok(display_string)
}

/// Unlock the DEK using a recovery key display string.
///
/// Parses the display string, derives the recovery key via HKDF, fetches the
/// recovery-encrypted DEK from the server, and decrypts it.
pub async fn unlock_with_recovery_key<E: UserEnv>(
    env: &E,
    recovery_key_string: &str,
) -> AppResult<Dek> {
    let rs = RecoverySecret::from_display_string(recovery_key_string)?;
    let rk = RecoveryKey::derive(&rs)?;

    debug!("Fetching recovery-encrypted DEK");
    let dek_response = env
        .encrypted_dek_api()
        .get(GetEncryptedDekRequest {
            key_type: DekKeyType::Recovery,
        })
        .await?;

    let encrypted_data: EncryptedData = dek_response.encrypted_key.try_into()?;
    let dek = rk.decrypt_dek(&encrypted_data)?;
    info!("DEK unlocked via recovery key");
    Ok(dek)
}

/// Re-encrypt the DEK with a new password-derived KEK and store it on the server.
///
/// This is used after recovery to set a new encryption password, or to change
/// the encryption password from settings.
pub async fn change_password<E: UserEnv>(env: &E, dek: &Dek, new_password: &str) -> AppResult<()> {
    let salt = SaltString::generate(&mut OsRng);
    let kek = Kek::derive(new_password, &salt)?;
    let encrypted = dek.encrypt_dek(&kek)?;

    env.encrypted_dek_api()
        .store(StoreEncryptedDekRequest {
            key_type: DekKeyType::Password,
            encrypted_key: encrypted.into(),
            salt: salt.to_string(),
        })
        .await?;

    info!("Password changed — DEK re-encrypted with new KEK");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::ports::api::{ApiClientError, ApiResult, user_api_port::UserApiPort};
    use api_types::auth::{LoginRequest, LoginResponse, Tokens};
    use api_types::email_verification::{
        ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
        VerifyEmailResponse,
    };
    use api_types::user::{AdminUserListResponse, UserInfo, UserRole};

    struct MockUserApi {
        should_fail: bool,
        fail_message: Option<String>,
    }

    impl MockUserApi {
        fn new() -> Self {
            Self {
                should_fail: false,
                fail_message: None,
            }
        }

        fn with_failure(message: &str) -> Self {
            Self {
                should_fail: true,
                fail_message: Some(message.to_string()),
            }
        }
    }

    #[async_trait]
    impl UserApiPort for MockUserApi {
        async fn create_user(&self, request: UserCreateRequest) -> ApiResult<UserCreateResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl(
                    self.fail_message.clone().unwrap_or_default(),
                ));
            }

            Ok(UserCreateResponse {
                id: Uuid::now_v7(),
                name: request.name,
                email: request.email,
                email_verified: true,
            })
        }

        async fn login(&self, _request: LoginRequest) -> ApiResult<api_types::auth::LoginResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl(
                    self.fail_message.clone().unwrap_or_default(),
                ));
            }

            Ok(api_types::auth::LoginResponse {
                access_token: "test-token".to_string(),
                refresh_token: "test-refresh".to_string(),
                user_id: Uuid::now_v7(),
                name: "Test User".to_string(),
                email: "test@example.com".to_string(),
                role: UserRole::User,
                subscription: Default::default(),
                encryption_setup_required: false,
            })
        }

        async fn get_me(&self, _: &str) -> ApiResult<UserInfo> {
            unimplemented!()
        }
        async fn list_all_users(
            &self,
            _: &str,
            _: u32,
            _: u32,
        ) -> ApiResult<AdminUserListResponse> {
            unimplemented!()
        }
        async fn verify_email(&self, _: VerifyEmailRequest) -> ApiResult<VerifyEmailResponse> {
            unimplemented!()
        }
        async fn resend_verification(
            &self,
            _: ResendVerificationRequest,
        ) -> ApiResult<ResendVerificationResponse> {
            unimplemented!()
        }

        async fn refresh(&self, _: &str) -> ApiResult<Tokens> {
            unimplemented!()
        }
    }

    struct MockEncryptedDekApi {
        should_fail: bool,
        storage: std::sync::Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, (api_types::common::Base64EncryptedData, String)>,
            >,
        >,
    }

    impl MockEncryptedDekApi {
        fn new(should_fail: bool) -> Self {
            Self {
                should_fail,
                storage: std::sync::Arc::new(std::sync::Mutex::new(
                    std::collections::HashMap::new(),
                )),
            }
        }
    }

    #[async_trait]
    impl crate::ports::api::encrypted_dek_api_port::EncryptedDekApiPort for MockEncryptedDekApi {
        async fn store(
            &self,
            request: api_types::encrypted_dek::StoreEncryptedDekRequest,
        ) -> ApiResult<api_types::encrypted_dek::StoreEncryptedDekResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("store failed".to_string()));
            }
            let key = request.key_type.to_string();
            self.storage
                .lock()
                .unwrap()
                .insert(key, (request.encrypted_key, request.salt));
            Ok(api_types::encrypted_dek::StoreEncryptedDekResponse { id: Uuid::now_v7() })
        }

        async fn get(
            &self,
            request: api_types::encrypted_dek::GetEncryptedDekRequest,
        ) -> ApiResult<api_types::encrypted_dek::GetEncryptedDekResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("get failed".to_string()));
            }
            let key = request.key_type.to_string();
            let storage = self.storage.lock().unwrap();
            match storage.get(&key) {
                Some((encrypted_key, salt)) => {
                    Ok(api_types::encrypted_dek::GetEncryptedDekResponse {
                        encrypted_key: encrypted_key.clone(),
                        salt: salt.clone(),
                    })
                }
                None => Err(ApiClientError::Api(api_types::error::ApiError::NotFound {
                    resource: format!("encrypted_dek/{key}"),
                })),
            }
        }
    }

    struct MockUserEnv {
        user_api: MockUserApi,
        dek_api: MockEncryptedDekApi,
    }

    impl MockUserEnv {
        fn new(user_api: MockUserApi, dek_api: MockEncryptedDekApi) -> Self {
            Self { user_api, dek_api }
        }
    }

    impl UserEnv for MockUserEnv {
        type UserApi = MockUserApi;
        type EncryptedDekApi = MockEncryptedDekApi;

        fn user_api(&self) -> &Self::UserApi {
            &self.user_api
        }

        fn encrypted_dek_api(&self) -> &Self::EncryptedDekApi {
            &self.dek_api
        }
    }

    #[tokio::test]
    async fn test_create_user_success() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let request = UserCreateRequest {
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = create_user(&env, request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.name, "Test User");
        assert_eq!(response.email, "test@example.com");
    }

    #[tokio::test]
    async fn test_create_user_failure() {
        let mock_api = MockUserApi::with_failure("user creation failed");
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let request = UserCreateRequest {
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = create_user(&env, request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_login_success() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let request = LoginRequest {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = login(&env, request).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_dek_success() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        let result = update_dek(&env, dek, "password123", DekKeyType::Password).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_dek_failure() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(true);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        let result = update_dek(&env, dek, "password123", DekKeyType::Password).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_login_failure() {
        let mock_api = MockUserApi::with_failure("invalid credentials");
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let request = LoginRequest {
            email: "test@example.com".to_string(),
            password: "wrong_password".to_string(),
        };

        let result = login(&env, request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_user_returns_valid_uuid() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let request = UserCreateRequest {
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = create_user(&env, request).await.unwrap();

        // Verify the UUID is a valid v7 (timestamp-based)
        assert_eq!(result.id.get_version(), Some(uuid::Version::SortRand));
    }

    #[tokio::test]
    async fn test_login_returns_tokens() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let request = LoginRequest {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = login(&env, request).await.unwrap();

        assert!(!result.access_token.is_empty());
        assert!(!result.refresh_token.is_empty());
    }

    #[tokio::test]
    async fn test_login_returns_user_info() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let request = LoginRequest {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = login(&env, request).await.unwrap();

        assert_eq!(result.name, "Test User");
        assert_eq!(result.email, "test@example.com");
        assert_eq!(result.role, UserRole::User);
    }

    // --- Recovery key tests ---

    #[tokio::test]
    async fn test_generate_recovery_key_success() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        let result = generate_recovery_key(&env, &dek).await;

        assert!(result.is_ok());
        let display = result.unwrap();
        assert!(display.starts_with("CLRK-"));
    }

    #[tokio::test]
    async fn test_generate_recovery_key_api_failure() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(true);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        let result = generate_recovery_key(&env, &dek).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unlock_with_recovery_key_success() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        let display = generate_recovery_key(&env, &dek).await.unwrap();

        let recovered = unlock_with_recovery_key(&env, &display).await.unwrap();
        assert_eq!(dek.key[..], recovered.key[..]);
    }

    #[tokio::test]
    async fn test_unlock_with_recovery_key_invalid_format() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let result = unlock_with_recovery_key(&env, "NOT-A-VALID-KEY").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unlock_with_recovery_key_wrong_key() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        let _display1 = generate_recovery_key(&env, &dek).await.unwrap();

        // Generate a different recovery key string (not stored)
        let other_rs = crate::domain::recovery_key::RecoverySecret::generate().unwrap();
        let other_display = other_rs.to_display_string();

        let result = unlock_with_recovery_key(&env, &other_display).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_change_password_success() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        // First store DEK with initial password
        update_dek(&env, dek.clone(), "old_password", DekKeyType::Password)
            .await
            .unwrap();

        let result = change_password(&env, &dek, "new_password").await;
        assert!(result.is_ok());

        // Verify the new password works
        let unlocked = unlock_dek(&env, "new_password", DekKeyType::Password)
            .await
            .unwrap();
        assert_eq!(dek.key[..], unlocked.key[..]);
    }

    #[tokio::test]
    async fn test_full_recovery_flow() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        // 1. Setup: generate DEK, store with password, generate recovery key
        let dek = Dek::generate().unwrap();
        update_dek(&env, dek.clone(), "original_password", DekKeyType::Password)
            .await
            .unwrap();
        let recovery_display = generate_recovery_key(&env, &dek).await.unwrap();

        // 2. Recovery: unlock DEK with recovery key
        let recovered_dek = unlock_with_recovery_key(&env, &recovery_display)
            .await
            .unwrap();
        assert_eq!(dek.key[..], recovered_dek.key[..]);

        // 3. Set new password
        change_password(&env, &recovered_dek, "new_password")
            .await
            .unwrap();

        // 4. Verify new password works
        let unlocked = unlock_dek(&env, "new_password", DekKeyType::Password)
            .await
            .unwrap();
        assert_eq!(dek.key[..], unlocked.key[..]);
    }

    #[tokio::test]
    async fn test_recovery_preserves_dek() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        let original_key = dek.key.clone();

        let recovery_display = generate_recovery_key(&env, &dek).await.unwrap();
        let recovered = unlock_with_recovery_key(&env, &recovery_display)
            .await
            .unwrap();

        // DEK bytes must be identical — no re-encryption of files needed
        assert_eq!(original_key[..], recovered.key[..]);
    }

    #[tokio::test]
    async fn test_key_rotation_preserves_dek() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();

        // Generate first recovery key
        let display1 = generate_recovery_key(&env, &dek).await.unwrap();

        // Rotate: generate new recovery key (overwrites via upsert)
        let display2 = generate_recovery_key(&env, &dek).await.unwrap();

        // New key works
        let recovered = unlock_with_recovery_key(&env, &display2).await.unwrap();
        assert_eq!(dek.key[..], recovered.key[..]);

        // Old key fails (encrypted blob was overwritten)
        let old_result = unlock_with_recovery_key(&env, &display1).await;
        assert!(old_result.is_err());
    }

    #[tokio::test]
    async fn test_existing_data_accessible_after_password_change() {
        use crate::domain::derived_keys::DerivedKeys;
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };

        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        update_dek(&env, dek.clone(), "old_password", DekKeyType::Password)
            .await
            .unwrap();

        // Encrypt test data with derived keys
        let keys_before = DerivedKeys::derive(&dek).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&keys_before.content_key[..]).unwrap();
        let nonce = Nonce::from_slice(b"test_nonce12");
        let test_data = b"important backup data";
        let encrypted = cipher.encrypt(nonce, test_data.as_ref()).unwrap();

        // Change password
        change_password(&env, &dek, "new_password").await.unwrap();

        // Unlock with new password and re-derive keys
        let unlocked = unlock_dek(&env, "new_password", DekKeyType::Password)
            .await
            .unwrap();
        let keys_after = DerivedKeys::derive(&unlocked).unwrap();

        // Derived keys must be identical since DEK is unchanged
        assert_eq!(keys_before.content_key[..], keys_after.content_key[..]);
        assert_eq!(keys_before.metadata_key[..], keys_after.metadata_key[..]);
        assert_eq!(keys_before.index_key[..], keys_after.index_key[..]);

        // Decrypt original data with new derived keys
        let cipher2 = Aes256Gcm::new_from_slice(&keys_after.content_key[..]).unwrap();
        let decrypted = cipher2.decrypt(nonce, encrypted.as_ref()).unwrap();
        assert_eq!(decrypted, test_data);
    }

    #[tokio::test]
    async fn test_existing_data_accessible_after_recovery_key_rotation() {
        use crate::domain::derived_keys::DerivedKeys;
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };

        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();

        // Encrypt test data with derived keys
        let keys = DerivedKeys::derive(&dek).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&keys.content_key[..]).unwrap();
        let nonce = Nonce::from_slice(b"test_nonce12");
        let test_data = b"important backup data";
        let encrypted = cipher.encrypt(nonce, test_data.as_ref()).unwrap();

        // Generate and then rotate recovery key
        let _display1 = generate_recovery_key(&env, &dek).await.unwrap();
        let display2 = generate_recovery_key(&env, &dek).await.unwrap();

        // Recover with new key
        let recovered = unlock_with_recovery_key(&env, &display2).await.unwrap();

        // DEK is unchanged — derived keys are identical
        let keys_after = DerivedKeys::derive(&recovered).unwrap();
        assert_eq!(keys.content_key[..], keys_after.content_key[..]);

        // Original data still decryptable
        let cipher2 = Aes256Gcm::new_from_slice(&keys_after.content_key[..]).unwrap();
        let decrypted = cipher2.decrypt(nonce, encrypted.as_ref()).unwrap();
        assert_eq!(decrypted, test_data);
    }

    #[tokio::test]
    async fn test_change_encryption_password_wrong_current() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        update_dek(&env, dek.clone(), "correct_password", DekKeyType::Password)
            .await
            .unwrap();

        // Try to unlock with wrong password — should fail
        let result = unlock_dek(&env, "wrong_password", DekKeyType::Password).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_password_rotation_invalidates_old_password() {
        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        update_dek(&env, dek.clone(), "old_password", DekKeyType::Password)
            .await
            .unwrap();

        // Verify old password works before rotation
        let unlocked = unlock_dek(&env, "old_password", DekKeyType::Password)
            .await
            .unwrap();
        assert_eq!(dek.key[..], unlocked.key[..]);

        // Rotate password
        change_password(&env, &dek, "new_password").await.unwrap();

        // Old password must fail
        let old_result = unlock_dek(&env, "old_password", DekKeyType::Password).await;
        assert!(old_result.is_err());

        // New password works and produces same DEK
        let new_unlocked = unlock_dek(&env, "new_password", DekKeyType::Password)
            .await
            .unwrap();
        assert_eq!(dek.key[..], new_unlocked.key[..]);
    }

    #[tokio::test]
    async fn test_recovery_then_password_change_invalidates_old_password() {
        use crate::domain::derived_keys::DerivedKeys;

        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        update_dek(&env, dek.clone(), "original_password", DekKeyType::Password)
            .await
            .unwrap();
        let recovery_display = generate_recovery_key(&env, &dek).await.unwrap();

        // Simulate "forgot password" — recover via recovery key
        let recovered_dek = unlock_with_recovery_key(&env, &recovery_display)
            .await
            .unwrap();

        // Set new password after recovery
        change_password(&env, &recovered_dek, "reset_password")
            .await
            .unwrap();

        // Original password must fail
        let old_result = unlock_dek(&env, "original_password", DekKeyType::Password).await;
        assert!(old_result.is_err());

        // New password works
        let unlocked = unlock_dek(&env, "reset_password", DekKeyType::Password)
            .await
            .unwrap();
        assert_eq!(dek.key[..], unlocked.key[..]);

        // Recovery key still works (independent of password)
        let recovered_again = unlock_with_recovery_key(&env, &recovery_display)
            .await
            .unwrap();
        assert_eq!(dek.key[..], recovered_again.key[..]);

        // Derived keys are identical across all unlock paths
        let keys_password = DerivedKeys::derive(&unlocked).unwrap();
        let keys_recovery = DerivedKeys::derive(&recovered_again).unwrap();
        assert_eq!(keys_password.content_key[..], keys_recovery.content_key[..]);
        assert_eq!(
            keys_password.metadata_key[..],
            keys_recovery.metadata_key[..]
        );
        assert_eq!(keys_password.index_key[..], keys_recovery.index_key[..]);
    }

    #[tokio::test]
    async fn test_password_and_recovery_key_rotation_together() {
        use crate::domain::derived_keys::DerivedKeys;
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };

        let mock_api = MockUserApi::new();
        let dek_api = MockEncryptedDekApi::new(false);
        let env = MockUserEnv::new(mock_api, dek_api);

        let dek = Dek::generate().unwrap();
        update_dek(&env, dek.clone(), "password_v1", DekKeyType::Password)
            .await
            .unwrap();
        let rk_v1 = generate_recovery_key(&env, &dek).await.unwrap();

        // Encrypt test data before any rotation
        let keys = DerivedKeys::derive(&dek).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&keys.content_key[..]).unwrap();
        let nonce = Nonce::from_slice(b"test_nonce12");
        let test_data = b"critical backup chunk data";
        let encrypted = cipher.encrypt(nonce, test_data.as_ref()).unwrap();

        // Rotate both password and recovery key
        change_password(&env, &dek, "password_v2").await.unwrap();
        let rk_v2 = generate_recovery_key(&env, &dek).await.unwrap();

        // Old credentials fail
        let old_pw = unlock_dek(&env, "password_v1", DekKeyType::Password).await;
        assert!(old_pw.is_err());
        let old_rk = unlock_with_recovery_key(&env, &rk_v1).await;
        assert!(old_rk.is_err());

        // New credentials work and produce same DEK
        let unlocked_pw = unlock_dek(&env, "password_v2", DekKeyType::Password)
            .await
            .unwrap();
        let unlocked_rk = unlock_with_recovery_key(&env, &rk_v2).await.unwrap();
        assert_eq!(dek.key[..], unlocked_pw.key[..]);
        assert_eq!(dek.key[..], unlocked_rk.key[..]);

        // Previously encrypted data is still decryptable via both paths
        let keys_pw = DerivedKeys::derive(&unlocked_pw).unwrap();
        let keys_rk = DerivedKeys::derive(&unlocked_rk).unwrap();

        let cipher_pw = Aes256Gcm::new_from_slice(&keys_pw.content_key[..]).unwrap();
        let decrypted_pw = cipher_pw.decrypt(nonce, encrypted.as_ref()).unwrap();
        assert_eq!(decrypted_pw, test_data);

        let cipher_rk = Aes256Gcm::new_from_slice(&keys_rk.content_key[..]).unwrap();
        let decrypted_rk = cipher_rk.decrypt(nonce, encrypted.as_ref()).unwrap();
        assert_eq!(decrypted_rk, test_data);
    }
}
