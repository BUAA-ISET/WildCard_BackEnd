use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::extract::FromRef;
use tokio::sync::RwLock;

use crate::domain::game::GameSession;
use crate::infrastructure::email::EmailSender;
use crate::infrastructure::user::UserRepository;
use crate::interface::replay::{ReplayPersistence, ReplayStore};
use crate::interface::report::ReportPersistence;
use crate::interface::room::RoomRepository;
use crate::interface::rule::{RulePersistence, RuleRepository};

pub type RuleStore = Arc<RwLock<RuleRepository>>;
pub type RoomStore = Arc<RwLock<RoomRepository>>;

#[derive(Clone)]
pub struct GlobalState {
    pub jwt_secret: JwtSecret,
    pub user: Arc<UserRepository>,
    pub verification_codes: Arc<RwLock<HashMap<String, VerificationCodeRecord>>>,
    pub games: Arc<RwLock<HashMap<String, GameSession>>>,
    pub rules: RuleStore,
    pub rooms: RoomStore,
    pub replays: ReplayStore,
    pub replay_persistence: ReplayPersistence,
    pub email: EmailSender,
    pub upload_dir: UploadDir,
}

#[derive(Clone)]
pub struct UploadDir(pub Arc<PathBuf>);

impl UploadDir {
    pub fn from_env() -> Self {
        let raw = std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string());
        Self(Arc::new(PathBuf::from(raw)))
    }
}

impl std::ops::Deref for UploadDir {
    type Target = PathBuf;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
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

impl FromRef<GlobalState> for RulePersistence {
    fn from_ref(input: &GlobalState) -> Self {
        RulePersistence {
            pool: input.user.pool.clone(),
        }
    }
}

impl FromRef<GlobalState> for ReportPersistence {
    fn from_ref(input: &GlobalState) -> Self {
        ReportPersistence {
            pool: input.user.pool.clone(),
        }
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

impl FromRef<GlobalState> for ReplayStore {
    fn from_ref(input: &GlobalState) -> Self {
        input.replays.clone()
    }
}

impl FromRef<GlobalState> for ReplayPersistence {
    fn from_ref(input: &GlobalState) -> Self {
        input.replay_persistence.clone()
    }
}

impl FromRef<GlobalState> for EmailSender {
    fn from_ref(input: &GlobalState) -> Self {
        input.email.clone()
    }
}

impl FromRef<GlobalState> for UploadDir {
    fn from_ref(input: &GlobalState) -> Self {
        input.upload_dir.clone()
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
