use std::{collections::HashMap, sync::Arc};

use axum::extract::FromRef;
use tokio::sync::RwLock;

use crate::domain::game::GameSession;
use crate::domain::room::{Room, RuleCatalogEntry};
use crate::infrastructure::user::UserRepository;

#[derive(Clone)]
pub struct GlobalState {
    pub jwt_secret: JwtSecret,
    pub user: Arc<UserRepository>,
    pub verification_codes: Arc<RwLock<HashMap<String, VerificationCodeRecord>>>,
    pub games: Arc<RwLock<HashMap<String, GameSession>>>,
    pub rules: RuleStore,
    pub rooms: RoomStore,
}

impl FromRef<GlobalState> for Arc<UserRepository> {
    fn from_ref(input: &GlobalState) -> Self {
        input.user.clone()
    }
}

impl FromRef<GlobalState> for Arc<RwLock<HashMap<String, VerificationCodeRecord>>> {
    fn from_ref(input: &GlobalState) -> Self {
        input.verification_codes.clone()
    }
}

impl FromRef<GlobalState> for RuleStore {
    fn from_ref(input: &GlobalState) -> Self {
        input.rules.clone()
    }
}

impl FromRef<GlobalState> for RoomStore {
    fn from_ref(input: &GlobalState) -> Self {
        input.rooms.clone()
    }
}

impl FromRef<GlobalState> for Arc<RwLock<HashMap<String, GameSession>>> {
    fn from_ref(input: &GlobalState) -> Self {
        input.games.clone()
    }
}

#[derive(Clone)]
pub struct JwtSecret(pub Vec<u8>);

impl FromRef<GlobalState> for JwtSecret {
    fn from_ref(input: &GlobalState) -> Self {
        input.jwt_secret.clone()
    }
}

#[derive(Clone, Debug)]
pub struct VerificationCodeRecord {
    pub code: String,
    pub expires_at_unix: i64,
}
