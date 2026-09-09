use api_types::remote_storage::{SftpAuthConfig, SftpCredentials, SftpHostKeyPolicy};
use cloudless_core::adapters::sftp_storage_adaptor::{
    SftpConnectionTestResult, SftpStorageAdaptor,
};
use cloudless_core::model::app_error::AppError;
use cloudless_core::model::file::ObjectKey;
use cloudless_core::ports::storage::StoragePort;
use rand::distr::{Alphanumeric, SampleString};

#[derive(Debug, Clone)]
struct SftpTestConfig {
    host: String,
    port: u16,
    username: String,
    password: Option<String>,
    private_key: Option<String>,
    private_key_passphrase: Option<String>,
    remote_root: String,
}

impl SftpTestConfig {
    fn from_env() -> Option<Self> {
        let host = std::env::var("CLOUDLESS_SFTP_TEST_HOST").ok()?;
        let username = std::env::var("CLOUDLESS_SFTP_TEST_USERNAME").ok()?;
        let remote_root = std::env::var("CLOUDLESS_SFTP_TEST_REMOTE_ROOT").ok()?;
        let port = std::env::var("CLOUDLESS_SFTP_TEST_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(22);

        Some(Self {
            host,
            port,
            username,
            password: std::env::var("CLOUDLESS_SFTP_TEST_PASSWORD").ok(),
            private_key: std::env::var("CLOUDLESS_SFTP_TEST_PRIVATE_KEY").ok(),
            private_key_passphrase: std::env::var("CLOUDLESS_SFTP_TEST_PRIVATE_KEY_PASSPHRASE")
                .ok(),
            remote_root,
        })
    }

    fn password_creds(&self, known_host_key: Option<String>) -> Option<SftpCredentials> {
        Some(SftpCredentials {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth: SftpAuthConfig::Password {
                password: self.password.clone()?,
            },
            remote_root_path: self.remote_root.clone(),
            host_key_policy: SftpHostKeyPolicy::Strict,
            known_host_key,
        })
    }

    fn private_key_creds(&self, known_host_key: Option<String>) -> Option<SftpCredentials> {
        Some(SftpCredentials {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth: SftpAuthConfig::PrivateKey {
                private_key_pem: self.private_key.clone()?,
                passphrase: self.private_key_passphrase.clone(),
            },
            remote_root_path: self.remote_root.clone(),
            host_key_policy: SftpHostKeyPolicy::Strict,
            known_host_key,
        })
    }
}

#[tokio::test]
#[ignore = "requires CLOUDLESS_SFTP_TEST_* environment and a reachable SFTP server"]
async fn password_auth_put_get_exists_list_delete() {
    let Some(config) = SftpTestConfig::from_env() else {
        eprintln!("skipping SFTP integration test: CLOUDLESS_SFTP_TEST_* env not configured");
        return;
    };
    let Some(initial_creds) = config.password_creds(None) else {
        eprintln!("skipping SFTP password integration test: password env not configured");
        return;
    };

    let test_result = SftpStorageAdaptor::test_connection(initial_creds)
        .await
        .expect("connection test should pass");
    assert_connection_test_result(&test_result);

    let creds = config
        .password_creds(Some(test_result.host_key_fingerprint))
        .unwrap();
    assert_storage_round_trip(creds).await;
}

#[tokio::test]
#[ignore = "requires CLOUDLESS_SFTP_TEST_* environment and a reachable SFTP server"]
async fn private_key_auth_put_get_exists_list_delete() {
    let Some(config) = SftpTestConfig::from_env() else {
        eprintln!("skipping SFTP integration test: CLOUDLESS_SFTP_TEST_* env not configured");
        return;
    };
    let Some(initial_creds) = config.private_key_creds(None) else {
        eprintln!("skipping SFTP private-key integration test: private key env not configured");
        return;
    };

    let test_result = SftpStorageAdaptor::test_connection(initial_creds)
        .await
        .expect("connection test should pass");
    assert_connection_test_result(&test_result);

    let creds = config
        .private_key_creds(Some(test_result.host_key_fingerprint))
        .unwrap();
    assert_storage_round_trip(creds).await;
}

#[tokio::test]
#[ignore = "requires CLOUDLESS_SFTP_TEST_* environment and a reachable SFTP server"]
async fn host_key_mismatch_fails_closed() {
    let Some(config) = SftpTestConfig::from_env() else {
        eprintln!("skipping SFTP integration test: CLOUDLESS_SFTP_TEST_* env not configured");
        return;
    };
    let Some(creds) = config.password_creds(Some("SHA256:not-the-real-host-key".to_string()))
    else {
        eprintln!("skipping SFTP host-key integration test: password env not configured");
        return;
    };

    let result = SftpStorageAdaptor::test_connection(creds).await;
    assert!(
        matches!(result, Err(AppError::Network { .. })),
        "expected host key mismatch to fail closed, got {result:?}"
    );
}

async fn assert_storage_round_trip(creds: SftpCredentials) {
    let adaptor = SftpStorageAdaptor::new(creds).expect("valid SFTP credentials");
    let test_id = random_suffix();
    let prefix = format!("{test_id}/integration/");
    let key = ObjectKey::new(format!("{prefix}chunk.bin"));
    let missing_key = ObjectKey::new(format!("{prefix}missing.bin"));
    let data = b"cloudless sftp integration".to_vec();

    assert!(!adaptor.exists(&key).await.expect("exists should work"));
    adaptor
        .put(&key, data.clone())
        .await
        .expect("put should work");
    assert!(adaptor.exists(&key).await.expect("exists should work"));
    assert_eq!(adaptor.get(&key).await.expect("get should work"), data);

    let listed = adaptor
        .list(&ObjectKey::new(prefix.clone()))
        .await
        .expect("list should work");
    assert!(
        listed.iter().any(|listed_key| listed_key == &key),
        "expected {key:?} in listed keys: {listed:?}"
    );

    let missing = adaptor.get(&missing_key).await;
    assert!(
        matches!(missing, Err(AppError::NotFound { .. })),
        "expected missing key to return NotFound, got {missing:?}"
    );

    adaptor.delete(&key).await.expect("delete should work");
    assert!(!adaptor.exists(&key).await.expect("exists should work"));
    adaptor
        .delete(&key)
        .await
        .expect("delete should be idempotent");
}

fn assert_connection_test_result(result: &SftpConnectionTestResult) {
    assert!(
        result.host_key_fingerprint.starts_with("SHA256:"),
        "unexpected fingerprint format: {}",
        result.host_key_fingerprint
    );
    assert!(result.root_writable);
}

fn random_suffix() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), 12)
}
