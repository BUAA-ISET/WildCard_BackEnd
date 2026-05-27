use crate::domain::game::GameSession;
use crate::infrastructure::email::EmailSender;
use crate::infrastructure::room::RoomRepository;
use crate::infrastructure::rule::RuleRepository;
use crate::infrastructure::user::UserRepository;
use axum::extract::FromRef;
use std::ops::Deref;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct GlobalState {
    pub jwt_secret: JwtSecret,
    pub user: Arc<UserRepository>,
    pub games: Arc<RwLock<HashMap<String, GameSession>>>,
    pub rules: Arc<RuleRepository>,
    pub rooms: Arc<RoomRepository>,
    pub email: Arc<EmailSender>,
    pub upload_dir: UploadDir,
}

impl FromRef<GlobalState> for Arc<UserRepository> {
    fn from_ref(input: &GlobalState) -> Self {
        input.user.clone()
    }
}

impl FromRef<GlobalState> for Arc<RuleRepository> {
    fn from_ref(input: &GlobalState) -> Self {
        input.rules.clone()
    }
}

impl FromRef<GlobalState> for Arc<RoomRepository> {
    fn from_ref(input: &GlobalState) -> Self {
        input.rooms.clone()
    }
}

impl FromRef<GlobalState> for Arc<RwLock<HashMap<String, GameSession>>> {
    fn from_ref(input: &GlobalState) -> Self {
        input.games.clone()
    }
}

impl FromRef<GlobalState> for Arc<EmailSender> {
    fn from_ref(input: &GlobalState) -> Self {
        input.email.clone()
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

#[derive(Clone)]
pub struct UploadDir(pub PathBuf);

impl Deref for UploadDir {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRef<GlobalState> for UploadDir {
    fn from_ref(input: &GlobalState) -> Self {
        input.upload_dir.clone()
    }
}
