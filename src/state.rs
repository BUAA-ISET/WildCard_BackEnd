use std::sync::Arc;

use axum::extract::FromRef;

use crate::infrastructure::{room::RoomRepository, user::UserRepository};

#[derive(Clone)]
pub struct GlobalState {
    pub jwt_secret: JwtSecret,
    pub user_repo: Arc<UserRepository>,
    pub room_repo: Arc<RoomRepository>,
}

#[derive(Clone)]
pub struct JwtSecret(pub Vec<u8>);

impl FromRef<GlobalState> for JwtSecret {
    fn from_ref(input: &GlobalState) -> Self {
        input.jwt_secret.clone()
    }
}

impl FromRef<GlobalState> for Arc<UserRepository> {
    fn from_ref(input: &GlobalState) -> Self {
        input.user_repo.clone()
    }
}

impl FromRef<GlobalState> for Arc<RoomRepository> {
    fn from_ref(input: &GlobalState) -> Self {
        input.room_repo.clone()
    }
}
