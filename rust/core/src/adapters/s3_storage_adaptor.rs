use std::error::Error;

use api_types::remote_storage::S3Credentials;
use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::delete_object::DeleteObjectError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::primitives::ByteStream;

use aws_smithy_http_client::tls;

use crate::model::app_error::AppError;
use crate::model::base::AppResult;
use crate::model::file::ObjectKey;
use crate::ports::storage::StoragePort;

/// Builds an HTTP client with bundled Mozilla root certificates instead of
/// relying on the system certificate store.
///
/// This is required for Android where `rustls-native-certs` cannot find system
/// root certificates and panics with "no valid root certificates parsed".
fn build_http_client() -> impl aws_sdk_s3::config::HttpClient {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    // Convert DER-encoded X.509 root certificates to a PEM bundle.
    // webpki-root-certs provides full certificates (not just trust anchors).
    let mut pem_bundle = Vec::new();
    for cert_der in webpki_root_certs::TLS_SERVER_ROOT_CERTS {
        pem_bundle.extend_from_slice(b"-----BEGIN CERTIFICATE-----\n");
        let b64 = STANDARD.encode(cert_der.as_ref());
        for chunk in b64.as_bytes().chunks(76) {
            pem_bundle.extend_from_slice(chunk);
            pem_bundle.push(b'\n');
        }
        pem_bundle.extend_from_slice(b"-----END CERTIFICATE-----\n");
    }

    let trust_store = tls::TrustStore::empty()
        .with_native_roots(false)
        .with_pem_certificate(pem_bundle);

    let tls_context = tls::TlsContext::builder()
        .with_trust_store(trust_store)
        .build()
        .expect("valid TLS context");

    aws_smithy_http_client::Builder::new()
        .tls_provider(tls::Provider::Rustls(
            tls::rustls_provider::CryptoMode::Ring,
        ))
        .tls_context(tls_context)
        .build_https()
}

#[derive(thiserror::Error, Debug)]
enum S3BackendError {
    #[error("Network error")]
    Network {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Other S3 error")]
    Other {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },
}

impl From<S3BackendError> for AppError {
    fn from(err: S3BackendError) -> Self {
        match err {
            S3BackendError::Network { source } => AppError::Network {
                message: "temporary s3 network failure".into(),
                source,
            },

            S3BackendError::Other { source } => AppError::Internal {
                message: "unexpected s3 error".into(),
                source,
            },
        }
    }
}

fn map_s3_error<E: Error + Send + Sync + 'static>(err: SdkError<E>) -> S3BackendError {
    tracing::error!("S3 error: {err}");
    match err {
        SdkError::ServiceError(_) => S3BackendError::Other {
            source: err.into_source().ok(),
        },
        _ => S3BackendError::Network {
            source: err.into_source().ok(),
        },
    }
}

impl From<SdkError<GetObjectError>> for S3BackendError {
    fn from(err: SdkError<GetObjectError>) -> Self {
        map_s3_error(err)
    }
}

impl From<SdkError<PutObjectError>> for S3BackendError {
    fn from(err: SdkError<PutObjectError>) -> Self {
        map_s3_error(err)
    }
}

impl From<SdkError<ListObjectsV2Error>> for S3BackendError {
    fn from(err: SdkError<ListObjectsV2Error>) -> Self {
        map_s3_error(err)
    }
}

impl From<SdkError<DeleteObjectError>> for S3BackendError {
    fn from(err: SdkError<DeleteObjectError>) -> Self {
        map_s3_error(err)
    }
}

impl From<SdkError<HeadObjectError>> for S3BackendError {
    fn from(err: SdkError<HeadObjectError>) -> Self {
        map_s3_error(err)
    }
}

pub struct S3StorageAdaptor {
    client: Client,
    bucket: String,
}

impl S3StorageAdaptor {
    /// Creates an adaptor from a pre-built S3 client and bucket name.
    ///
    /// Use this when the client is configured via the default AWS credential
    /// chain (e.g. `~/.aws/credentials`, `AWS_PROFILE`, instance metadata).
    pub fn from_client(client: Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// Creates an adaptor using explicit static credentials.
    ///
    /// Useful for client-side apps (Tauri/mobile) where user-provided
    /// credentials are passed directly.
    pub async fn new(s3_creds: S3Credentials) -> AppResult<Self> {
        let region = Region::new(s3_creds.region.clone());
        let creds = Credentials::new(&s3_creds.access_key, &s3_creds.secret, None, None, "static");

        // Build S3 client config directly instead of using aws_config::defaults().load().
        // Two issues with the defaults loader on Android:
        // 1. It probes the EC2 IMDS endpoint (169.254.169.254) which hangs on Android.
        // 2. The default TLS client uses rustls-native-certs which panics on Android
        //    because there are no system root certificates at the expected paths.
        //
        // We solve both by building the config directly and providing a custom HTTP client
        // that bundles Mozilla's root certificates via webpki-roots.
        let http_client = build_http_client();

        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::v2025_08_07())
            .region(region)
            .credentials_provider(creds)
            .http_client(http_client)
            .build();

        let client = Client::from_conf(s3_config);
        Ok(Self {
            client,
            bucket: s3_creds.bucket,
        })
    }
}

#[async_trait]
impl StoragePort for S3StorageAdaptor {
    async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(S3BackendError::from)?;
        Ok(())
    }

    async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(S3BackendError::from)?;

        let data = resp
            .body
            .collect()
            .await
            .map_err(|e| S3BackendError::Network {
                source: Some(e.into()),
            })?
            .to_vec();
        Ok(data)
    }

    async fn delete(&self, key: &ObjectKey) -> AppResult<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(S3BackendError::from)?;
        Ok(())
    }

    async fn list(&self, prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
        let mut out = Vec::new();

        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix.as_str())
            .send()
            .await
            .map_err(S3BackendError::from)?;

        if let Some(contents) = resp.contents {
            for obj in contents {
                if let Some(key) = obj.key {
                    out.push(ObjectKey::new(key));
                }
            }
        }
        Ok(out)
    }

    async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
        let resp = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await;
        match resp {
            Ok(_) => Ok(true),
            Err(e) => match &e {
                SdkError::ServiceError(err) => {
                    if err.err().is_not_found() {
                        Ok(false)
                    } else {
                        Err(map_s3_error(e).into())
                    }
                }
                _ => Err(map_s3_error(e).into()),
            },
        }
    }
}
