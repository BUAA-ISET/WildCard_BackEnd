#![allow(dead_code)]

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    routing::{get, post},
};
use jsonwebtoken::{EncodingKey, Header};
use serde_json::{Value, json};
use tokio::{sync::RwLock, task::JoinSet};
use tower::ServiceExt;
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
            banned: bool,
            banned_until: Option<i64>,
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
                    banned: self.banned,
                    banned_until: self.banned_until,
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
                                        banned: user.banned,
                                        banned_until: None,
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
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/auth.rs"
        ));
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

    use axum::extract::FromRef;
    use tokio::sync::RwLock;

    use crate::{
        TestState,
        infrastructure::user::UserRepository,
        interface::replay::{ReplayPersistence, ReplayStore},
    };

    #[derive(Clone, Debug)]
    pub struct JwtSecret(pub Vec<u8>);

    pub type RoomStore = Arc<RwLock<crate::interface::room::RoomRepository>>;
    pub type RuleStore = Arc<RwLock<crate::interface::rule::RuleRepository>>;

    impl FromRef<TestState> for JwtSecret {
        fn from_ref(input: &TestState) -> Self {
            input.jwt_secret.clone()
        }
    }

    impl FromRef<TestState> for Arc<UserRepository> {
        fn from_ref(input: &TestState) -> Self {
            input.user.clone()
        }
    }

    impl FromRef<TestState> for RuleStore {
        fn from_ref(input: &TestState) -> Self {
            input.rules.clone()
        }
    }

    impl FromRef<TestState> for RoomStore {
        fn from_ref(input: &TestState) -> Self {
            input.rooms.clone()
        }
    }

    impl FromRef<TestState> for ReplayStore {
        fn from_ref(input: &TestState) -> Self {
            input.replays.clone()
        }
    }

    impl FromRef<TestState> for ReplayPersistence {
        fn from_ref(input: &TestState) -> Self {
            input.replay_persistence.clone()
        }
    }
}

use domain::{
    rule_engine::{ExportedRuleDesign, RuleEngine},
    user::{User, UserId},
};
use infrastructure::user::UserRepository;
use interface::{
    auth::TokenClaims,
    replay::{ReplayPersistence, ReplayStore, build_replay_store},
    room::{choose_action, create_room, current_game, join_room, set_ready, start_game},
    rule::{PublishedRule, RuleRepository},
};
use state::{JwtSecret, RoomStore, RuleStore};

#[derive(Clone)]
struct TestState {
    jwt_secret: JwtSecret,
    user: Arc<UserRepository>,
    rules: RuleStore,
    rooms: RoomStore,
    replays: ReplayStore,
    replay_persistence: ReplayPersistence,
}

#[derive(Clone)]
struct Account {
    id: Uuid,
    token: String,
}

#[derive(Debug)]
struct RequestOutcome {
    label: String,
    status: StatusCode,
    elapsed: Duration,
    body: Value,
}

#[derive(Debug)]
struct PressureStats {
    total: usize,
    ok: usize,
    failed: usize,
    wall: Duration,
    min: Duration,
    max: Duration,
    avg: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
    qps: f64,
    statuses: BTreeMap<u16, usize>,
}

#[derive(Debug, Clone)]
struct StartedSession {
    label: String,
    room_code: String,
    session_id: String,
    action_id: String,
    actor_token: String,
    first_card_id: String,
}

fn replay_persistence() -> ReplayPersistence {
    ReplayPersistence {
        pool: sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_millis(1))
            .connect_lazy("postgres://user:password@127.0.0.1:1/wildcard_test")
            .expect("lazy replay pool should be constructible"),
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

fn user(id: Uuid, name: &str) -> User {
    User {
        id: UserId(id),
        name: name.to_string(),
        email: format!("{name}@example.com"),
        password: "hashed".to_string(),
        avatar: String::new(),
        role: "user".to_string(),
        banned: false,
        banned_until: None,
    }
}

fn test_state(secret: Vec<u8>, users: Vec<User>) -> TestState {
    TestState {
        jwt_secret: JwtSecret(secret),
        user: Arc::new(UserRepository::with_users(users)),
        rules: Arc::new(RwLock::new(RuleRepository {
            published: HashMap::from([("tiny".to_string(), published_rule("tiny", "Tiny"))]),
        })),
        rooms: Arc::new(RwLock::new(interface::room::RoomRepository::default())),
        replays: build_replay_store(),
        replay_persistence: replay_persistence(),
    }
}

fn app(state: TestState) -> Router {
    Router::new()
        .route("/api/room/create", post(create_room))
        .route("/api/room/join", post(join_room))
        .route("/api/room/current/ready", post(set_ready))
        .route("/api/room/current/start", post(start_game))
        .route("/api/games/current", get(current_game))
        .route(
            "/api/games/{sessionId}/actions/{actionId}/choose",
            post(choose_action),
        )
        .with_state(state)
}

fn auth_token(secret: &[u8], user_id: Uuid) -> String {
    jsonwebtoken::encode(
        &Header::default(),
        &TokenClaims {
            user_id: UserId(user_id),
            iat: 0,
            exp: usize::MAX,
        },
        &EncodingKey::from_secret(secret),
    )
    .expect("token should encode")
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn create_accounts(secret: &[u8], prefix: &str, count: usize) -> (Vec<User>, Vec<Account>) {
    let mut users = Vec::with_capacity(count);
    let mut accounts = Vec::with_capacity(count);
    for idx in 0..count {
        let id = Uuid::new_v4();
        users.push(user(id, &format!("{prefix}-{idx}")));
        accounts.push(Account {
            id,
            token: auth_token(secret, id),
        });
    }
    (users, accounts)
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&bytes).to_string() }))
    }
}

async fn request_json(
    app: Router,
    label: impl Into<String>,
    method: &str,
    uri: String,
    token: String,
    body: Option<Value>,
) -> RequestOutcome {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let request = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        builder.body(Body::from(body.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let started = Instant::now();
    let response = app.oneshot(request).await.expect("request should resolve");
    let elapsed = started.elapsed();
    let status = response.status();
    let body = response_json(response).await;

    RequestOutcome {
        label: label.into(),
        status,
        elapsed,
        body,
    }
}

fn micros(duration: Duration) -> u128 {
    duration.as_micros()
}

fn duration_from_micros(micros: u128) -> Duration {
    Duration::from_micros(micros as u64)
}

fn percentile(sorted_micros: &[u128], pct: usize) -> Duration {
    if sorted_micros.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted_micros.len() - 1) * pct) / 100;
    duration_from_micros(sorted_micros[index])
}

fn summarize(outcomes: &[RequestOutcome], wall: Duration) -> PressureStats {
    let mut values = outcomes
        .iter()
        .map(|outcome| micros(outcome.elapsed))
        .collect::<Vec<_>>();
    values.sort_unstable();
    let total_micros = values.iter().copied().sum::<u128>();
    let statuses = outcomes.iter().fold(BTreeMap::new(), |mut acc, outcome| {
        *acc.entry(outcome.status.as_u16()).or_insert(0) += 1;
        acc
    });

    PressureStats {
        total: outcomes.len(),
        ok: outcomes
            .iter()
            .filter(|outcome| outcome.status.is_success())
            .count(),
        failed: outcomes
            .iter()
            .filter(|outcome| !outcome.status.is_success())
            .count(),
        wall,
        min: values
            .first()
            .copied()
            .map(duration_from_micros)
            .unwrap_or_default(),
        max: values
            .last()
            .copied()
            .map(duration_from_micros)
            .unwrap_or_default(),
        avg: if values.is_empty() {
            Duration::ZERO
        } else {
            duration_from_micros(total_micros / values.len() as u128)
        },
        p50: percentile(&values, 50),
        p95: percentile(&values, 95),
        p99: percentile(&values, 99),
        qps: if wall.is_zero() {
            0.0
        } else {
            outcomes.len() as f64 / wall.as_secs_f64()
        },
        statuses,
    }
}

fn fmt_duration(duration: Duration) -> String {
    format!("{:.2}ms", duration.as_secs_f64() * 1_000.0)
}

fn print_report(name: &str, stats: &PressureStats, outcomes: &[RequestOutcome]) {
    println!("\n== {name} ==");
    println!(
        "total={} ok={} failed={} wall={} qps={:.2}",
        stats.total,
        stats.ok,
        stats.failed,
        fmt_duration(stats.wall),
        stats.qps
    );
    println!(
        "latency min={} avg={} p50={} p95={} p99={} max={}",
        fmt_duration(stats.min),
        fmt_duration(stats.avg),
        fmt_duration(stats.p50),
        fmt_duration(stats.p95),
        fmt_duration(stats.p99),
        fmt_duration(stats.max)
    );

    for (status, count) in &stats.statuses {
        println!("status[{status}]={count}");
    }

    for outcome in outcomes
        .iter()
        .filter(|outcome| !outcome.status.is_success())
        .take(3)
    {
        println!(
            "failure label={} status={} body={}",
            outcome.label, outcome.status, outcome.body
        );
    }
}

fn assert_success(name: &str, outcomes: &[RequestOutcome]) {
    let failed = outcomes
        .iter()
        .filter(|outcome| !outcome.status.is_success())
        .collect::<Vec<_>>();

    assert!(
        failed.is_empty(),
        "{name} should have no failed requests, first failure: {:?}",
        failed
            .first()
            .map(|outcome| (&outcome.label, outcome.status, &outcome.body))
    );
}

async fn prepare_started_sessions(
    app: &Router,
    hosts: &[Account],
    guests: &[Account],
    session_count: usize,
) -> Vec<StartedSession> {
    let mut sessions = Vec::with_capacity(session_count);

    for idx in 0..session_count {
        let host = &hosts[idx];
        let guest = &guests[idx];

        let created = request_json(
            app.clone(),
            format!("setup-create-{idx}"),
            "POST",
            "/api/room/create".to_string(),
            host.token.clone(),
            Some(json!({
                "ruleId": "tiny",
                "roundTime": 15,
                "password": null
            })),
        )
        .await;
        assert_eq!(
            created.status,
            StatusCode::OK,
            "room setup create should succeed"
        );
        let room_code = created.body["data"]["code"]
            .as_str()
            .expect("create response should include room code")
            .to_string();

        let joined = request_json(
            app.clone(),
            format!("setup-join-{idx}"),
            "POST",
            "/api/room/join".to_string(),
            guest.token.clone(),
            Some(json!({
                "code": room_code,
                "password": null
            })),
        )
        .await;
        assert_eq!(
            joined.status,
            StatusCode::OK,
            "room setup join should succeed"
        );

        let ready = request_json(
            app.clone(),
            format!("setup-ready-{idx}"),
            "POST",
            "/api/room/current/ready".to_string(),
            guest.token.clone(),
            Some(json!({ "isReady": true })),
        )
        .await;
        assert_eq!(
            ready.status,
            StatusCode::OK,
            "room setup ready should succeed"
        );

        let started = request_json(
            app.clone(),
            format!("setup-start-{idx}"),
            "POST",
            "/api/room/current/start".to_string(),
            host.token.clone(),
            None,
        )
        .await;
        assert_eq!(
            started.status,
            StatusCode::OK,
            "room setup start should succeed"
        );
        let session_id = started.body["data"]["gameSessionId"]
            .as_str()
            .expect("start response should include session id")
            .to_string();

        let host_snapshot = request_json(
            app.clone(),
            format!("setup-snapshot-host-{idx}"),
            "GET",
            format!("/api/games/current?roomCode={room_code}"),
            host.token.clone(),
            None,
        )
        .await;
        assert_eq!(
            host_snapshot.status,
            StatusCode::OK,
            "host snapshot should succeed"
        );

        let pending_player_id = host_snapshot.body["data"]["pendingAction"]["playerId"]
            .as_str()
            .expect("snapshot should include pending player id")
            .to_string();
        let action_id = host_snapshot.body["data"]["pendingAction"]["actionId"]
            .as_str()
            .expect("snapshot should include action id")
            .to_string();

        let (actor_token, actor_snapshot) = if pending_player_id == host.id.to_string() {
            (host.token.clone(), host_snapshot.body)
        } else {
            let guest_snapshot = request_json(
                app.clone(),
                format!("setup-snapshot-guest-{idx}"),
                "GET",
                format!("/api/games/current?roomCode={room_code}"),
                guest.token.clone(),
                None,
            )
            .await;
            assert_eq!(
                guest_snapshot.status,
                StatusCode::OK,
                "guest snapshot should succeed"
            );
            (guest.token.clone(), guest_snapshot.body)
        };

        let first_card_id = actor_snapshot["data"]["handCards"]
            .as_array()
            .and_then(|cards| cards.first())
            .and_then(|card| card.get("id"))
            .and_then(Value::as_str)
            .expect("actor snapshot should expose first hand card")
            .to_string();

        sessions.push(StartedSession {
            label: format!("session-{idx}"),
            room_code,
            session_id,
            action_id,
            actor_token,
            first_card_id,
        });
    }

    sessions
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "manual pressure test; run with --ignored --nocapture"]
async fn room_lifecycle_pressure_prints_report() {
    let room_count = env_usize("WC_PRESSURE_ROOM_COUNT", 48);
    println!("room pressure config: room_count={room_count}");

    let secret = b"room-pressure-secret".to_vec();
    let (host_users, hosts) = create_accounts(&secret, "room-host", room_count);
    let (guest_users, guests) = create_accounts(&secret, "room-guest", room_count);
    let state = test_state(secret, host_users.into_iter().chain(guest_users).collect());
    let app = app(state);

    let mut create_set = JoinSet::new();
    let create_started = Instant::now();
    for (idx, host) in hosts.iter().enumerate() {
        let app = app.clone();
        let token = host.token.clone();
        create_set.spawn(async move {
            request_json(
                app,
                format!("create-{idx}"),
                "POST",
                "/api/room/create".to_string(),
                token,
                Some(json!({
                    "ruleId": "tiny",
                    "roundTime": 20,
                    "password": null
                })),
            )
            .await
        });
    }

    let mut created = Vec::with_capacity(room_count);
    while let Some(result) = create_set.join_next().await {
        created.push(result.expect("create task should finish"));
    }
    let create_stats = summarize(&created, create_started.elapsed());
    print_report("room/create", &create_stats, &created);
    assert_success("room/create", &created);

    let room_codes = created
        .iter()
        .map(|outcome| {
            outcome.body["data"]["code"]
                .as_str()
                .expect("create response should include room code")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        room_codes.len(),
        room_count,
        "each room create should yield a code"
    );

    let mut join_set = JoinSet::new();
    let join_started = Instant::now();
    for (idx, (guest, room_code)) in guests.iter().zip(room_codes.iter()).enumerate() {
        let app = app.clone();
        let token = guest.token.clone();
        let room_code = room_code.clone();
        join_set.spawn(async move {
            request_json(
                app,
                format!("join-{idx}"),
                "POST",
                "/api/room/join".to_string(),
                token,
                Some(json!({
                    "code": room_code,
                    "password": null
                })),
            )
            .await
        });
    }

    let mut joined = Vec::with_capacity(room_count);
    while let Some(result) = join_set.join_next().await {
        joined.push(result.expect("join task should finish"));
    }
    let join_stats = summarize(&joined, join_started.elapsed());
    print_report("room/join", &join_stats, &joined);
    assert_success("room/join", &joined);
    assert!(joined.iter().all(|outcome| {
        outcome.body["data"]["players"]
            .as_array()
            .is_some_and(|players| players.len() == 2)
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "manual pressure test; run with --ignored --nocapture"]
async fn game_snapshot_and_action_pressure_prints_report() {
    let session_count = env_usize("WC_PRESSURE_GAME_SESSION_COUNT", 24);
    let reads_per_session = env_usize("WC_PRESSURE_SNAPSHOT_READS_PER_SESSION", 8);
    println!(
        "game pressure config: session_count={session_count} reads_per_session={reads_per_session}"
    );

    let secret = b"game-pressure-secret".to_vec();
    let (host_users, hosts) = create_accounts(&secret, "game-host", session_count);
    let (guest_users, guests) = create_accounts(&secret, "game-guest", session_count);
    let state = test_state(secret, host_users.into_iter().chain(guest_users).collect());
    let app = app(state);
    let sessions = prepare_started_sessions(&app, &hosts, &guests, session_count).await;

    let mut snapshot_set = JoinSet::new();
    let snapshot_started = Instant::now();
    for session in &sessions {
        for iteration in 0..reads_per_session {
            let app = app.clone();
            let token = session.actor_token.clone();
            let room_code = session.room_code.clone();
            let label = format!("{}-snapshot-{iteration}", session.label);
            snapshot_set.spawn(async move {
                request_json(
                    app,
                    label,
                    "GET",
                    format!("/api/games/current?roomCode={room_code}"),
                    token,
                    None,
                )
                .await
            });
        }
    }

    let mut snapshots = Vec::with_capacity(session_count * reads_per_session);
    while let Some(result) = snapshot_set.join_next().await {
        snapshots.push(result.expect("snapshot task should finish"));
    }
    let snapshot_stats = summarize(&snapshots, snapshot_started.elapsed());
    print_report("game/current", &snapshot_stats, &snapshots);
    assert_success("game/current", &snapshots);
    assert!(
        snapshots
            .iter()
            .all(|outcome| outcome.body["data"]["status"] == "playing")
    );

    let mut action_set = JoinSet::new();
    let action_started = Instant::now();
    for session in &sessions {
        let app = app.clone();
        let token = session.actor_token.clone();
        let session_id = session.session_id.clone();
        let action_id = session.action_id.clone();
        let first_card_id = session.first_card_id.clone();
        let label = format!("{}-choose", session.label);
        action_set.spawn(async move {
            request_json(
                app,
                label,
                "POST",
                format!("/api/games/{session_id}/actions/{action_id}/choose"),
                token,
                Some(json!({
                    "cards": [first_card_id],
                    "choice": null
                })),
            )
            .await
        });
    }

    let mut actions = Vec::with_capacity(session_count);
    while let Some(result) = action_set.join_next().await {
        actions.push(result.expect("action task should finish"));
    }
    let action_stats = summarize(&actions, action_started.elapsed());
    print_report("game/choose", &action_stats, &actions);
    assert_success("game/choose", &actions);
    assert!(
        actions
            .iter()
            .all(|outcome| outcome.body["data"]["status"] == "finished")
    );
}
