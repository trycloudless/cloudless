use uuid::Uuid;

pub struct DbId(pub String);
pub struct Email(pub String);

pub enum UserAccountStatus {
    Active,
    Blocked(String),
    EmailVerificationPending,
}

pub struct S3Credentials {
    pub access_key: String,
    pub secret: String,
    pub region: String,
    pub bucket: String,
}

pub enum VendorAuthType {
    S3 {
        access_key: String,
        secret: String,
        region: String,
    },
    GoogleDrive {
        access_token: String,
        // refresh_token: String,
        // expiry: String,
    },
}

pub struct VendorAuthDetails {
    pub id: String,
    pub name: String,
    pub vendor_type: VendorAuthType,
}

pub struct UserEntity {
    pub id: Uuid,
    pub email: Email,
    pub status: UserAccountStatus,
    pub vendor_auth_details: Option<Vec<VendorAuthDetails>>,
}
