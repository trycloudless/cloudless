use api_types::user::UserRole;
use sqlx::{
    Postgres,
    encode::IsNull,
    postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef},
};

/// Infra-layer newtype bridging `UserRole` and the `user_role` Postgres enum.
/// Used as the type annotation in `sqlx::query!` overrides so that the column
/// decodes directly to `UserRole` without a manual parse function.
#[derive(Debug)]
pub struct PgUserRole(pub UserRole);

impl From<PgUserRole> for UserRole {
    fn from(r: PgUserRole) -> Self {
        r.0
    }
}

impl sqlx::Type<Postgres> for PgUserRole {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("user_role")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for PgUserRole {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let role = match <&str as sqlx::Decode<Postgres>>::decode(value)? {
            "SUPER_ADMIN" => UserRole::SuperAdmin,
            _ => UserRole::User,
        };
        Ok(PgUserRole(role))
    }
}

impl sqlx::Encode<'_, Postgres> for PgUserRole {
    fn encode_by_ref(
        &self,
        buf: &mut PgArgumentBuffer,
    ) -> Result<IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let s = match self.0 {
            UserRole::User => "USER",
            UserRole::SuperAdmin => "SUPER_ADMIN",
        };
        <&str as sqlx::Encode<Postgres>>::encode(s, buf)
    }
}
