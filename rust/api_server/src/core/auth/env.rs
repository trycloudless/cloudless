use crate::core::{auth::jwt_service::JwtService, ports};

pub trait AuthEnv {
    type AuthRepo: ports::AuthRepo;
    type RefreshTokenRepo: ports::RefreshTokenRepo;

    fn auth_repo(&self) -> &Self::AuthRepo;
    fn refresh_token_repo(&self) -> &Self::RefreshTokenRepo;
    fn jwt_service(&self) -> &JwtService;
}
