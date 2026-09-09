use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::Base64EncryptedData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DekKeyType {
    Password,
    Recovery,
}

impl std::fmt::Display for DekKeyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password => write!(f, "password"),
            Self::Recovery => write!(f, "recovery"),
        }
    }
}

impl std::str::FromStr for DekKeyType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "password" => Ok(Self::Password),
            "recovery" => Ok(Self::Recovery),
            _ => Err(format!("unknown dek key type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreEncryptedDekRequest {
    pub key_type: DekKeyType,
    pub encrypted_key: Base64EncryptedData,
    pub salt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreEncryptedDekResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetEncryptedDekRequest {
    pub key_type: DekKeyType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetEncryptedDekResponse {
    pub encrypted_key: Base64EncryptedData,
    pub salt: String,
}
