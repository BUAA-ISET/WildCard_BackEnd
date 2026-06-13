#![allow(dead_code)]

use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use tokio::sync::RwLock;
use uuid::Uuid;

mod error {
    pub use wildcard_backend::error::*;
}

mod domain {
    pub mod replay {
        pub use wildcard_backend::domain::replay::*;
    }

    pub mod room {
        pub use wildcard_backend::domain::room::*;
    }
    pub mod rule_engine {
        pub use wildcard_backend::domain::rule_engine::*;
    }
    pub mod user {
        pub use wildcard_backend::domain::user::*;
    }
}

mod infrastructure {
    pub mod user {
        use std::{collections::HashMap, sync::Arc};

        use tokio::sync::RwLock;
        use uuid::Uuid;

        use crate::{
            domain::user::{User, UserId},
            error::AppError,
        };

        #[derive(Clone)]
        struct StoredUser {
            id: Uuid,
            name: String,
            email: String,
            password: String,
            avatar: String,
            role: String,
        }

        impl StoredUser {
            fn into_user(self) -> User {
                User {
                    id: UserId(self.id),
                    name: self.name,
                    email: self.email,
                    password: self.password,
                    avatar: self.avatar,
                    role: self.role,
                    banned: false,
                }
            }
        }

        #[derive(Default)]
        pub struct UserRepository {
            users: Arc<RwLock<HashMap<Uuid, StoredUser>>>,
        }

        impl std::fmt::Debug for UserRepository {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("UserRepository").finish_non_exhaustive()
            }
        }

        impl UserRepository {
            pub fn with_users(users: Vec<User>) -> Self {
                Self {
                    users: Arc::new(RwLock::new(
                        users
                            .into_iter()
                            .map(|user| {
                                (
                                    user.id.0,
                                    StoredUser {
                                        id: user.id.0,
                                        name: user.name,
                                        email: user.email,
                                        password: user.password,
                                        avatar: user.avatar,
                                        role: user.role,
                                    },
                                )
                            })
                            .collect(),
                    )),
                }
            }

            pub async fn find_by_id(&self, user_id: &UserId) -> Result<Option<User>, AppError> {
                Ok(self
                    .users
                    .read()
                    .await
                    .get(&user_id.0)
                    .cloned()
                    .map(StoredUser::into_user))
            }
        }
    }
}

mod interface {
    pub mod auth {
        use serde::{Deserialize, Serialize};

        use crate::domain::user::UserId;

        #[derive(Debug, Serialize, Deserialize)]
        pub struct TokenClaims {
            #[serde(rename = "sub")]
            pub user_id: UserId,
            pub iat: usize,
            pub exp: usize,
        }
    }

    pub mod user {
        use serde::Serialize;

        #[derive(Debug, Serialize)]
        pub struct ApiResponse<T> {
            pub success: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub data: Option<T>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub message: Option<String>,
        }

        impl<T> ApiResponse<T> {
            pub(crate) fn success(data: T) -> Self {
                Self {
                    success: true,
                    data: Some(data),
                    message: None,
                }
            }

            pub(crate) fn success_with_optional_data(data: Option<T>) -> Self {
                Self {
                    success: true,
                    data,
                    message: None,
                }
            }
        }

        impl ApiResponse<()> {
            pub(crate) fn success_without_data(message: Option<String>) -> Self {
                Self {
                    success: true,
                    data: None,
                    message,
                }
            }
        }
    }

    pub mod rule {
        use std::collections::HashMap;

        use crate::domain::rule_engine::{ExportedRuleDesign, RuntimeRule};

        #[derive(Debug, Clone)]
        pub struct PublishedRule {
            pub id: String,
            pub owner_id: String,
            pub name: String,
            pub player_count: u8,
            pub description: String,
            pub version: u32,
            pub design: ExportedRuleDesign,
            pub runtime: RuntimeRule,
            pub created_at: i64,
            pub updated_at: i64,
            pub introduction: String,
            pub cover_url: String,
            pub screenshot_urls: Vec<String>,
        }

        #[derive(Debug, Default)]
        pub struct RuleRepository {
            pub published: HashMap<String, PublishedRule>,
        }
    }

    pub mod replay {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/replay.rs"
        ));
    }

    pub mod room {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/room.rs"
        ));
    }
}

mod state {
    use std::sync::Arc;

    use tokio::sync::RwLock;
    pub type RoomStore = Arc<RwLock<crate::interface::room::RoomRepository>>;
    pub type RuleStore = Arc<RwLock<crate::interface::rule::RuleRepository>>;
}

use domain::{
    room::{Player, Room, RoomStatus},
    rule_engine::{ExportedRuleDesign, PlayerActionInput, RuleEngine},
    user::{User, UserId},
};
use infrastructure::user::UserRepository;
use interface::{
    auth::TokenClaims,
    replay::{ReplayPersistence, build_replay_store},
    room::{
        CreateRoomRequest, CurrentGameQuery, JoinRoomRequest, ReadyRequest, RoomCodeQuery,
        RuleQuery, build_room_store, check_password, choose_action, create_room, current_game,
        current_room, get_game, get_room_rule, join_room, leave_room, play_cards, set_ready,
        skip_action, start_game,
    },
    rule::{PublishedRule, RuleRepository},
};
use state::RuleStore;

fn replay_persistence() -> ReplayPersistence {
    ReplayPersistence {
        pool: sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_millis(10))
            .connect_lazy("postgres://user:password@127.0.0.1:1/wildcard_test")
            .expect("lazy replay pool should be constructible"),
    }
}

fn claims(id: Uuid) -> TokenClaims {
    TokenClaims {
        user_id: UserId(id),
        iat: 0,
        exp: usize::MAX,
    }
}

fn user(id: Uuid, name: &str) -> User {
    User {
        id: UserId(id),
        name: name.to_string(),
        email: format!("{name}@example.com"),
        password: "hashed".to_string(),
        avatar: format!("/{name}.png"),
        role: "user".to_string(),
        banned: false,
    }
}

fn player(id: &str, ready: bool, joined_at: i64) -> Player {
    Player {
        id: id.to_string(),
        username: format!("user-{id}"),
        avatar: String::new(),
        is_ready: ready,
        joined_at: Some(joined_at),
    }
}

fn valid_design() -> ExportedRuleDesign {
    let content = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test2.json"),
    )
    .expect("test fixture should exist");
    serde_json::from_str(&content).expect("test fixture should parse")
}

fn published_rule(id: &str, name: &str) -> PublishedRule {
    let design = valid_design();
    let runtime = RuleEngine::parse(name.to_string(), 2, "desc".to_string(), design.clone())
        .expect("fixture rule should compile");
    PublishedRule {
        id: id.to_string(),
        owner_id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        player_count: 2,
        description: "desc".to_string(),
        version: 1,
        design,
        runtime,
        created_at: 1,
        updated_at: 1,
        introduction: "intro".to_string(),
        cover_url: "/static/rule-images/cover.png".to_string(),
        screenshot_urls: vec!["/static/rule-images/shot.png".to_string()],
    }
}

fn rule_store(rule: PublishedRule) -> RuleStore {
    Arc::new(RwLock::new(RuleRepository {
        published: HashMap::from([(rule.id.clone(), rule)]),
    }))
}

fn room(code: &str, host_id: &str, players: Vec<Player>) -> Room {
    Room {
        id: format!("room-{code}"),
        code: code.to_string(),
        host_id: host_id.to_string(),
        player_count: 2,
        round_time: 20,
        rule_id: "tiny".to_string(),
        rule_name: "Tiny".to_string(),
        password: None,
        has_password: false,
        players,
        status: RoomStatus::Waiting,
        game_session_id: None,
    }
}

#[test]
fn domain_room_catalog_and_serialization_cover_public_contracts() {
    let catalog = wildcard_backend::domain::room::default_rule_catalog();
    assert!(catalog.contains_key("classic"));
    assert!(catalog.contains_key("party"));
    assert_eq!(catalog["classic"].option.player_count, 4);

    let value = serde_json::to_value(wildcard_backend::domain::room::Room {
        id: "room-1".to_string(),
        code: "ABC123".to_string(),
        host_id: "host".to_string(),
        player_count: 2,
        round_time: 30,
        rule_id: "rule-1".to_string(),
        rule_name: "Rule".to_string(),
        password: Some("secret".to_string()),
        has_password: true,
        players: vec![wildcard_backend::domain::room::Player {
            id: "host".to_string(),
            username: "host-user".to_string(),
            avatar: String::new(),
            is_ready: true,
            joined_at: Some(10),
        }],
        status: wildcard_backend::domain::room::RoomStatus::Playing,
        game_session_id: Some("session-1".to_string()),
    })
    .expect("room should serialize");

    assert_eq!(value["hostId"], "host");
    assert_eq!(value["hasPassword"], true);
    assert_eq!(value["players"][0]["isReady"], true);
    assert_eq!(value["status"], "playing");
    assert_eq!(value["gameSessionId"], "session-1");
}

#[tokio::test]
async fn room_lifecycle_endpoints_cover_create_join_ready_rule_and_leave() {
    let host = Uuid::new_v4();
    let guest = Uuid::new_v4();
    let user_repo = Arc::new(UserRepository::with_users(vec![
        user(host, "host"),
        user(guest, "guest"),
    ]));
    let room_store = build_room_store();
    let rules = rule_store(published_rule("tiny", "Tiny"));
    let replay_store = build_replay_store();
    let replay_persistence = replay_persistence();

    let created = create_room(
        claims(host),
        State(user_repo.clone()),
        State(rules.clone()),
        State(room_store.clone()),
        Json(CreateRoomRequest {
            rule_id: "tiny".to_string(),
            round_time: 30,
            password: Some(" secret ".to_string()),
        }),
    )
    .await
    .expect("host should create room")
    .0
    .data
    .expect("room should be returned");
    assert_eq!(created.password, None);
    assert!(created.has_password);

    let code = created.code.clone();
    let password = check_password(
        State(room_store.clone()),
        Query(RoomCodeQuery {
            code: Some(code.to_lowercase()),
        }),
    )
    .await
    .0;
    assert!(password.has_password);

    let wrong_password = join_room(
        claims(guest),
        State(user_repo.clone()),
        State(room_store.clone()),
        Json(JoinRoomRequest {
            code: code.clone(),
            password: Some("bad".to_string()),
        }),
    )
    .await;
    assert!(wrong_password.is_err());

    let joined = join_room(
        claims(guest),
        State(user_repo.clone()),
        State(room_store.clone()),
        Json(JoinRoomRequest {
            code: code.clone(),
            password: Some("secret".to_string()),
        }),
    )
    .await
    .expect("guest should join")
    .0
    .data
    .expect("joined room should be returned");
    assert_eq!(joined.players.len(), 2);

    let current_by_member = current_room(
        claims(guest),
        State(room_store.clone()),
        Query(RoomCodeQuery { code: None }),
    )
    .await
    .expect("current room should resolve by membership")
    .0
    .data
    .flatten()
    .expect("room should be present");
    assert_eq!(current_by_member.code, code);

    let current_by_code = current_room(
        claims(guest),
        State(room_store.clone()),
        Query(RoomCodeQuery {
            code: Some(code.to_lowercase()),
        }),
    )
    .await
    .expect("current room should resolve by code")
    .0
    .data
    .flatten()
    .expect("room should be present");
    assert_eq!(current_by_code.players.len(), 2);

    let host_unready = set_ready(
        claims(host),
        State(room_store.clone()),
        Json(ReadyRequest { is_ready: false }),
    )
    .await
    .expect("host cannot become unready while waiting")
    .0
    .data
    .expect("room should be returned");
    assert!(
        host_unready
            .players
            .iter()
            .any(|p| p.id == host.to_string() && p.is_ready)
    );

    let _ = set_ready(
        claims(guest),
        State(room_store.clone()),
        Json(ReadyRequest { is_ready: true }),
    )
    .await
    .expect("guest should ready");

    let rule_response = get_room_rule(
        claims(guest),
        State(rules.clone()),
        State(room_store.clone()),
        Query(RuleQuery { room_id: None }),
    )
    .await
    .expect("member should read room rule")
    .0
    .data
    .expect("rule should be returned");
    assert!(rule_response.rule["classes"].is_object());

    let started = start_game(
        claims(host),
        State(rules.clone()),
        State(room_store.clone()),
        State(replay_store.clone()),
        State(replay_persistence.clone()),
    )
    .await
    .expect("host should start full ready room")
    .0
    .data
    .expect("started room should be returned");
    assert_eq!(started.status, RoomStatus::Playing);
    assert!(started.game_session_id.is_some());
    let started_session_id = started
        .game_session_id
        .clone()
        .expect("started room should have session id");
    let replay_guard = replay_store.read().await;
    let replay_id = replay_guard
        .session_replay_ids
        .get(&started_session_id)
        .expect("started session should be linked to replay");
    let replay = replay_guard
        .replays
        .get(replay_id)
        .expect("started replay should be stored in memory");
    assert_eq!(replay.record.room_code, code);
    assert_eq!(replay.record.players.len(), 2);
    assert_eq!(replay.frames.len(), 1);
    drop(replay_guard);

    let leave_started = leave_room(claims(guest), State(room_store.clone()))
        .await
        .expect("guest can leave started room")
        .0;
    assert!(leave_started.success);

    let guard = room_store.read().await;
    let room = guard.rooms.get(&code).expect("room remains for host");
    assert_eq!(room.status, RoomStatus::Waiting);
    assert_eq!(room.game_session_id, None);
}

#[tokio::test]
async fn room_game_queries_and_actions_cover_snapshot_error_and_finish_paths() {
    let host = Uuid::new_v4();
    let guest = Uuid::new_v4();
    let user_repo = Arc::new(UserRepository::with_users(vec![
        user(host, "host"),
        user(guest, "guest"),
    ]));
    let room_store = build_room_store();
    let rules = rule_store(published_rule("tiny", "Tiny"));
    let replay_store = build_replay_store();
    let replay_persistence = replay_persistence();

    let created = create_room(
        claims(host),
        State(user_repo.clone()),
        State(rules.clone()),
        State(room_store.clone()),
        Json(CreateRoomRequest {
            rule_id: "tiny".to_string(),
            round_time: 15,
            password: None,
        }),
    )
    .await
    .expect("host should create room")
    .0
    .data
    .expect("room should be returned");

    let _ = join_room(
        claims(guest),
        State(user_repo.clone()),
        State(room_store.clone()),
        Json(JoinRoomRequest {
            code: created.code.clone(),
            password: None,
        }),
    )
    .await
    .expect("guest should join");
    let _ = set_ready(
        claims(guest),
        State(room_store.clone()),
        Json(ReadyRequest { is_ready: true }),
    )
    .await
    .expect("guest should ready");

    let not_host = start_game(
        claims(guest),
        State(rules.clone()),
        State(room_store.clone()),
        State(replay_store.clone()),
        State(replay_persistence.clone()),
    )
    .await;
    assert!(not_host.is_err());

    let started = start_game(
        claims(host),
        State(rules.clone()),
        State(room_store.clone()),
        State(replay_store.clone()),
        State(replay_persistence.clone()),
    )
    .await
    .expect("host should start room")
    .0
    .data
    .expect("started room should be returned");
    let session_id = started
        .game_session_id
        .clone()
        .expect("session id should exist");

    let snapshot = current_game(
        claims(host),
        State(rules.clone()),
        State(room_store.clone()),
        Query(CurrentGameQuery {
            room_code: Some(created.code.to_lowercase()),
        }),
    )
    .await
    .expect("current game should resolve by room code")
    .0
    .data
    .expect("snapshot should be returned");
    assert_eq!(snapshot.status, "playing");
    assert_eq!(snapshot.round_time, 15);
    assert!(snapshot.pending_action.is_some());
    assert!(!snapshot.hand_cards.is_empty());

    let direct_snapshot = get_game(
        claims(host),
        State(rules.clone()),
        State(room_store.clone()),
        Path(session_id.clone()),
    )
    .await
    .expect("snapshot should resolve by session id")
    .0
    .data
    .expect("snapshot should be returned");
    assert_eq!(direct_snapshot.session_id, session_id);

    let wrong_action = play_cards(
        claims(host),
        State(rules.clone()),
        State(room_store.clone()),
        State(replay_store.clone()),
        State(replay_persistence.clone()),
        Path((session_id.clone(), "not-the-pending-action".to_string())),
        Json(PlayerActionInput {
            cards: Vec::new(),
            choice: None,
        }),
    )
    .await;
    assert!(wrong_action.is_err());

    let pending = direct_snapshot
        .pending_action
        .expect("pending action should exist");
    let first_card = direct_snapshot
        .hand_cards
        .first()
        .expect("host should have a card")
        .id
        .clone();
    let finished = choose_action(
        claims(host),
        State(rules.clone()),
        State(room_store.clone()),
        State(replay_store.clone()),
        State(replay_persistence.clone()),
        Path((session_id.clone(), pending.action_id.clone())),
        Json(PlayerActionInput {
            cards: vec![first_card],
            choice: None,
        }),
    )
    .await
    .expect("valid action should finish tiny rule")
    .0
    .data
    .expect("finished snapshot should be returned");
    assert_eq!(finished.status, "finished");
    assert_eq!(finished.last_action.as_ref().unwrap().action, "play_cards");
    assert_eq!(finished.winner_ids, vec![host.to_string()]);
    let replay_guard = replay_store.read().await;
    let replay_id = replay_guard
        .session_replay_ids
        .get(&session_id)
        .expect("session should be linked to a replay");
    let replay = replay_guard
        .replays
        .get(replay_id)
        .expect("replay should remain in memory after finishing");
    assert_eq!(replay.record.winner_ids, vec![host.to_string()]);
    assert!(replay.frames.len() >= 2);
    assert_eq!(
        replay
            .frames
            .last()
            .unwrap()
            .action
            .as_ref()
            .unwrap()
            .player_id,
        host.to_string()
    );
    drop(replay_guard);

    let after_finish = current_game(
        claims(guest),
        State(rules.clone()),
        State(room_store.clone()),
        Query(CurrentGameQuery {
            room_code: Some(started.code),
        }),
    )
    .await
    .expect("finished session should still be queryable")
    .0
    .data
    .expect("snapshot should be returned");
    assert_eq!(after_finish.status, "finished");

    let skip_after_finished = skip_action(
        claims(guest),
        State(rules.clone()),
        State(room_store.clone()),
        State(replay_store.clone()),
        State(replay_persistence.clone()),
        Path((session_id, pending.action_id)),
    )
    .await;
    assert!(skip_after_finished.is_err());
}

#[tokio::test]
async fn room_repository_paths_cover_host_rotation_full_rooms_and_missing_data() {
    let host = Uuid::new_v4().to_string();
    let guest = Uuid::new_v4().to_string();
    let third = Uuid::new_v4();
    let room_store = build_room_store();
    {
        let mut guard = room_store.write().await;
        guard.player_rooms.insert(host.clone(), "ROOMX".to_string());
        guard
            .player_rooms
            .insert(guest.clone(), "ROOMX".to_string());
        guard.rooms.insert(
            "ROOMX".to_string(),
            room(
                "ROOMX",
                &host,
                vec![player(&host, true, 20), player(&guest, false, 10)],
            ),
        );
    }

    let users = Arc::new(UserRepository::with_users(vec![user(third, "third")]));
    let full_join = join_room(
        claims(third),
        State(users),
        State(room_store.clone()),
        Json(JoinRoomRequest {
            code: "ROOMX".to_string(),
            password: None,
        }),
    )
    .await;
    assert!(full_join.is_err());

    let _ = leave_room(
        claims(Uuid::parse_str(&host).unwrap()),
        State(room_store.clone()),
    )
    .await
    .expect("host can leave");
    {
        let guard = room_store.read().await;
        let room = guard.rooms.get("ROOMX").expect("guest keeps room alive");
        assert_eq!(room.host_id, guest);
        assert!(room.players.iter().all(|player| player.is_ready));
    }

    let missing_current = current_room(
        claims(third),
        State(room_store.clone()),
        Query(RoomCodeQuery { code: None }),
    )
    .await
    .expect("missing membership returns empty success")
    .0
    .data
    .flatten();
    assert!(missing_current.is_none());

    let missing_password = check_password(
        State(room_store.clone()),
        Query(RoomCodeQuery {
            code: Some("missing".to_string()),
        }),
    )
    .await
    .0;
    assert!(!missing_password.has_password);

    let _ = leave_room(
        claims(Uuid::parse_str(&guest).unwrap()),
        State(room_store.clone()),
    )
    .await
    .expect("last player can leave");
    let guard = room_store.read().await;
    assert!(!guard.rooms.contains_key("ROOMX"));
}
