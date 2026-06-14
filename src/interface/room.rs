use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        room::{Player, Room, RoomRuleResponse, RoomStatus},
        rule_engine::{GameCard, GameSession, PlayerActionInput, RuleAssets, RuleEngine},
        user::UserId,
    },
    error::AppError,
    infrastructure::user::UserRepository,
    interface::{
        auth::TokenClaims,
        replay::{
            ReplayPersistence, ReplayStore, append_match_replay_frame_with_persistence,
            start_match_replay_with_persistence,
        },
        user::ApiResponse,
    },
    state::{RoomStore, RuleStore},
};

#[derive(Debug, Default)]
pub struct RoomRepository {
    pub rooms: HashMap<String, Room>,
    pub player_rooms: HashMap<String, String>,
    pub sessions: HashMap<String, GameSession>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    #[serde(rename = "roundTime")]
    pub round_time: u16,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JoinRoomRequest {
    pub code: String,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReadyRequest {
    #[serde(rename = "isReady")]
    pub is_ready: bool,
}

#[derive(Debug, Deserialize)]
pub struct RoomCodeQuery {
    pub code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckPasswordResponse {
    pub success: bool,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
}

#[derive(Debug, Deserialize)]
pub struct RuleQuery {
    pub room_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CurrentGameQuery {
    #[serde(rename = "roomCode")]
    pub room_code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCardDisplay {
    pub rank: String,
    pub suit: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCardView {
    pub id: String,
    pub properties: HashMap<String, i64>,
    pub display: GameCardDisplay,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePlayerView {
    pub id: String,
    pub username: String,
    pub avatar: String,
    pub card_count: usize,
    pub public_properties: HashMap<String, i64>,
    pub online: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameTableView {
    pub played_cards: Vec<GameCardView>,
    pub public_properties: HashMap<String, i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingActionView {
    pub action_id: String,
    #[serde(rename = "type")]
    pub action_type: String,
    pub player_id: String,
    pub timer: u64,
    pub deadline_at: Option<i64>,
    pub can_skip: bool,
    pub options: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameActionRecordView {
    pub player_id: String,
    pub action: String,
    pub cards: Vec<GameCardView>,
    pub message: String,
    pub turn: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshotView {
    pub session_id: String,
    pub room_code: String,
    pub rule_id: String,
    pub status: String,
    pub current_player_id: String,
    pub round_time: u32,
    pub deadline_at: Option<i64>,
    pub players: Vec<GamePlayerView>,
    pub table: GameTableView,
    pub hand_cards: Vec<GameCardView>,
    pub pending_action: Option<PendingActionView>,
    pub last_action: Option<GameActionRecordView>,
    pub winner_ids: Vec<String>,
    pub assets: RuleAssets,
}

pub fn build_room_store() -> RoomStore {
    Arc::new(tokio::sync::RwLock::new(RoomRepository::default()))
}

pub async fn create_room(
    claims: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    Json(payload): Json<CreateRoomRequest>,
) -> Result<Json<ApiResponse<Room>>, AppError> {
    let player = player_from_claims(&user_repo, &claims, true).await?;
    let player_id = player.id.clone();
    let published_rule = {
        let rule_guard = rule_store.read().await;
        rule_guard
            .published
            .get(&payload.rule_id)
            .cloned()
            .ok_or(AppError::NotFound)?
    };

    let mut guard = room_store.write().await;
    remove_player_from_existing_room(&mut guard, &player_id);

    let code = generate_room_code(&guard.rooms);
    let password = normalize_password(payload.password);
    let room = Room {
        id: uuid::Uuid::new_v4().to_string(),
        code: code.clone(),
        host_id: player_id.clone(),
        player_count: published_rule.player_count as usize,
        round_time: payload.round_time as u32,
        rule_id: published_rule.id.clone(),
        rule_name: published_rule.name.clone(),
        has_password: password.is_some(),
        password,
        players: vec![player],
        status: RoomStatus::Waiting,
        game_session_id: None,
    };

    guard.player_rooms.insert(player_id, code.clone());
    guard.rooms.insert(code, room.clone());

    Ok(Json(ApiResponse::success(public_room(&room))))
}

pub async fn join_room(
    claims: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    State(room_store): State<RoomStore>,
    Json(payload): Json<JoinRoomRequest>,
) -> Result<Json<ApiResponse<Room>>, AppError> {
    let player = player_from_claims(&user_repo, &claims, false).await?;
    let player_id = player.id.clone();
    let room_code = payload.code.trim().to_uppercase();
    let mut guard = room_store.write().await;
    remove_player_from_existing_room(&mut guard, &player_id);

    let room = guard.rooms.get_mut(&room_code).ok_or(AppError::NotFound)?;
    if !matches!(room.status, RoomStatus::Waiting) {
        return Err(AppError::InvalidInput("房间已经开始".to_string()));
    }
    if room
        .password
        .as_deref()
        .is_some_and(|password| Some(password) != payload.password.as_deref())
    {
        return Err(AppError::InvalidInput("房间密码错误".to_string()));
    }
    if !room.players.iter().any(|player| player.id == player_id)
        && room.players.len() >= room.player_count
    {
        return Err(AppError::InvalidInput("房间已满".to_string()));
    }

    if !room.players.iter().any(|player| player.id == player_id) {
        room.players.push(player);
    }

    let room = public_room(room);
    guard.player_rooms.insert(player_id, room.code.clone());
    Ok(Json(ApiResponse::success(room)))
}

pub async fn check_password(
    State(room_store): State<RoomStore>,
    Query(query): Query<RoomCodeQuery>,
) -> Json<CheckPasswordResponse> {
    let has_password = if let Some(code) = query.code {
        room_store
            .read()
            .await
            .rooms
            .get(&code.trim().to_uppercase())
            .and_then(|room| room.password.as_ref())
            .is_some()
    } else {
        false
    };

    Json(CheckPasswordResponse {
        success: true,
        has_password,
    })
}

pub async fn current_room(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_store): State<RoomStore>,
    Query(query): Query<RoomCodeQuery>,
) -> Result<Json<ApiResponse<Option<Room>>>, AppError> {
    let player_id = user_id.to_string();
    let guard = room_store.read().await;
    let room = if let Some(code) = query.code {
        guard.rooms.get(&code.trim().to_uppercase()).cloned()
    } else {
        guard
            .player_rooms
            .get(&player_id)
            .and_then(|code| guard.rooms.get(code))
            .cloned()
    };

    Ok(Json(ApiResponse::success_with_optional_data(Some(
        room.map(|room| public_room(&room)),
    ))))
}

pub async fn set_ready(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_store): State<RoomStore>,
    Json(payload): Json<ReadyRequest>,
) -> Result<Json<ApiResponse<Room>>, AppError> {
    let player_id = user_id.to_string();
    let mut guard = room_store.write().await;
    let room_code = guard
        .player_rooms
        .get(&player_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    let room = guard.rooms.get_mut(&room_code).ok_or(AppError::NotFound)?;

    if !matches!(room.status, RoomStatus::Waiting) {
        return Err(AppError::InvalidInput(
            "对局已开始，不能修改准备状态".to_string(),
        ));
    }

    let player = room
        .players
        .iter_mut()
        .find(|player| player.id == player_id)
        .ok_or(AppError::NotFound)?;
    if room.host_id == player_id && !payload.is_ready {
        player.is_ready = true;
    } else {
        player.is_ready = payload.is_ready;
    }

    Ok(Json(ApiResponse::success(public_room(room))))
}

pub async fn start_game(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    State(replay_store): State<ReplayStore>,
    State(replay_persistence): State<ReplayPersistence>,
) -> Result<Json<ApiResponse<Room>>, AppError> {
    let player_id = user_id.to_string();
    let mut room_guard = room_store.write().await;
    let room_code = room_guard
        .player_rooms
        .get(&player_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    let room = room_guard
        .rooms
        .get_mut(&room_code)
        .ok_or(AppError::NotFound)?;

    if room.host_id != player_id {
        return Err(AppError::Unauthorized("只有房主可以开始游戏".to_string()));
    }
    if room.players.len() != room.player_count || !room.players.iter().all(|player| player.is_ready)
    {
        return Err(AppError::InvalidInput(
            "房间必须满员且所有玩家已准备".to_string(),
        ));
    }
    refresh_room_player_profiles(&user_repo, room).await;

    let runtime_rule = {
        let rule_guard = rule_store.read().await;
        let published = rule_guard
            .published
            .get(&room.rule_id)
            .ok_or(AppError::NotFound)?;
        published.runtime.clone()
    };
    let player_ids = room
        .players
        .iter()
        .map(|player| player.id.clone())
        .collect();
    let session = RuleEngine::start_session(room.code.clone(), &runtime_rule, player_ids)?;
    let session_id = session.id.clone();

    let room_snapshot = {
        room.status = RoomStatus::Playing;
        room.game_session_id = Some(session_id.clone());
        public_room(room)
    };
    start_match_replay_with_persistence(
        &replay_store,
        Some(&replay_persistence),
        &session,
        &room_snapshot,
    )
    .await;
    room_guard.sessions.insert(session_id, session);

    Ok(Json(ApiResponse::success(room_snapshot)))
}

pub async fn get_room_rule(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    Query(query): Query<RuleQuery>,
) -> Result<Json<ApiResponse<RoomRuleResponse>>, AppError> {
    let player_id = user_id.to_string();
    let guard = room_store.read().await;
    let room = resolve_room_for_rule_query(&guard, &player_id, query.room_id.as_deref())?;
    let rule_id = room.rule_id.clone();
    let room_id = room.id.clone();
    drop(guard);

    let rule_guard = rule_store.read().await;
    let published = rule_guard
        .published
        .get(&rule_id)
        .ok_or(AppError::NotFound)?;

    Ok(Json(ApiResponse::success(RoomRuleResponse {
        room_id,
        rule: serde_json::to_value(&published.design)
            .map_err(|error| AppError::InvalidInput(error.to_string()))?,
    })))
}

pub async fn current_game(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    Query(query): Query<CurrentGameQuery>,
) -> Result<Json<ApiResponse<GameSnapshotView>>, AppError> {
    let guard = room_store.read().await;
    let room_code = query
        .room_code
        .as_deref()
        .map(str::trim)
        .map(str::to_uppercase)
        .ok_or(AppError::NotFound)?;

    let session = if let Some(session_id) = guard
        .rooms
        .get(&room_code)
        .and_then(|room| room.game_session_id.clone())
    {
        guard.sessions.get(&session_id).cloned()
    } else {
        guard
            .sessions
            .values()
            .find(|session| session.room_code == room_code && session.status == "finished")
            .cloned()
    }
    .ok_or(AppError::NotFound)?;

    let rule_id = guard
        .rooms
        .get(&room_code)
        .map(|room| room.rule_id.clone())
        .unwrap_or_default();
    let round_time = guard
        .rooms
        .get(&room_code)
        .map(|room| room.round_time)
        .unwrap_or(30);
    let assets = load_rule_assets(&rule_store, &rule_id).await;

    Ok(Json(ApiResponse::success(build_game_snapshot(
        &session,
        &rule_id,
        round_time,
        &user_id.to_string(),
        guard.rooms.get(&room_code),
        assets,
    ))))
}

pub async fn get_game(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<GameSnapshotView>>, AppError> {
    let guard = room_store.read().await;
    let session = guard
        .sessions
        .get(&session_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    let room = guard.rooms.get(&session.room_code);
    let rule_id = room.map(|room| room.rule_id.clone()).unwrap_or_default();
    let round_time = room.map(|room| room.round_time).unwrap_or(30);
    let assets = load_rule_assets(&rule_store, &rule_id).await;

    Ok(Json(ApiResponse::success(build_game_snapshot(
        &session,
        &rule_id,
        round_time,
        &user_id.to_string(),
        room,
        assets,
    ))))
}

pub async fn play_cards(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    State(replay_store): State<ReplayStore>,
    State(replay_persistence): State<ReplayPersistence>,
    Path((session_id, action_id)): Path<(String, String)>,
    Json(payload): Json<PlayerActionInput>,
) -> Result<Json<ApiResponse<GameSnapshotView>>, AppError> {
    submit_game_action(
        user_id.to_string(),
        rule_store,
        room_store,
        replay_store,
        replay_persistence,
        session_id,
        action_id,
        payload,
    )
    .await
}

pub async fn skip_action(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    State(replay_store): State<ReplayStore>,
    State(replay_persistence): State<ReplayPersistence>,
    Path((session_id, action_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<GameSnapshotView>>, AppError> {
    submit_game_action(
        user_id.to_string(),
        rule_store,
        room_store,
        replay_store,
        replay_persistence,
        session_id,
        action_id,
        PlayerActionInput {
            cards: Vec::new(),
            choice: None,
        },
    )
    .await
}

pub async fn choose_action(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    State(replay_store): State<ReplayStore>,
    State(replay_persistence): State<ReplayPersistence>,
    Path((session_id, action_id)): Path<(String, String)>,
    Json(payload): Json<PlayerActionInput>,
) -> Result<Json<ApiResponse<GameSnapshotView>>, AppError> {
    submit_game_action(
        user_id.to_string(),
        rule_store,
        room_store,
        replay_store,
        replay_persistence,
        session_id,
        action_id,
        payload,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn submit_game_action(
    player_id: String,
    rule_store: RuleStore,
    room_store: RoomStore,
    replay_store: ReplayStore,
    replay_persistence: ReplayPersistence,
    session_id: String,
    action_id: String,
    payload: PlayerActionInput,
) -> Result<Json<ApiResponse<GameSnapshotView>>, AppError> {
    let mut room_guard = room_store.write().await;
    let room_code = room_guard
        .sessions
        .get(&session_id)
        .map(|session| session.room_code.clone())
        .ok_or(AppError::NotFound)?;
    let room = room_guard.rooms.get(&room_code).ok_or(AppError::NotFound)?;
    let runtime_rule = {
        let rule_guard = rule_store.read().await;
        rule_guard
            .published
            .get(&room.rule_id)
            .map(|published| published.runtime.clone())
            .ok_or(AppError::NotFound)?
    };

    let session_snapshot = {
        let session = room_guard
            .sessions
            .get_mut(&session_id)
            .ok_or(AppError::NotFound)?;
        let pending_id = session
            .pending_action
            .as_ref()
            .map(|action| action.id.clone())
            .ok_or_else(|| AppError::InvalidInput("当前没有等待中的动作".to_string()))?;
        if pending_id != action_id {
            return Err(AppError::InvalidInput("动作 ID 不匹配".to_string()));
        }

        RuleEngine::submit_action(&runtime_rule, session, &player_id, payload)?;
        session.clone()
    };

    let room_before_reset = room_guard.rooms.get(&room_code).cloned();
    append_match_replay_frame_with_persistence(
        &replay_store,
        Some(&replay_persistence),
        &session_snapshot,
        room_before_reset.as_ref(),
    )
    .await;

    if session_snapshot.status == "finished"
        && let Some(room) = room_guard.rooms.get_mut(&room_code)
    {
        reset_room_after_game(room);
    }

    let room = room_guard.rooms.get(&room_code);
    let rule_id = room.map(|room| room.rule_id.clone()).unwrap_or_default();
    let round_time = room.map(|room| room.round_time).unwrap_or(30);
    let assets = runtime_rule.design.assets.clone();

    Ok(Json(ApiResponse::success(build_game_snapshot(
        &session_snapshot,
        &rule_id,
        round_time,
        &player_id,
        room,
        assets,
    ))))
}

pub async fn leave_room(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_store): State<RoomStore>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let player_id = user_id.to_string();
    let mut guard = room_store.write().await;
    if let Some(code) = guard.player_rooms.remove(&player_id) {
        remove_player_from_room(&mut guard, &code, &player_id);
    }

    Ok(Json(ApiResponse::success_without_data(Some(
        "已离开房间".to_string(),
    ))))
}

fn resolve_room_for_rule_query<'a>(
    guard: &'a RoomRepository,
    player_id: &str,
    room_id: Option<&str>,
) -> Result<&'a Room, AppError> {
    if let Some(room_id) = room_id {
        let room_id = room_id.trim();
        return guard
            .rooms
            .values()
            .find(|room| room.id == room_id || room.code == room_id.to_uppercase())
            .ok_or(AppError::NotFound);
    }

    let room_code = guard
        .player_rooms
        .get(player_id)
        .ok_or(AppError::NotFound)?;
    guard.rooms.get(room_code).ok_or(AppError::NotFound)
}

fn remove_player_from_existing_room(guard: &mut RoomRepository, player_id: &str) {
    if let Some(old_code) = guard.player_rooms.remove(player_id) {
        remove_player_from_room(guard, &old_code, player_id);
    }
}

fn remove_player_from_room(guard: &mut RoomRepository, code: &str, player_id: &str) {
    let mut should_remove_room = false;
    let mut should_remove_session = false;
    let mut session_id_to_remove = None;

    if let Some(room) = guard.rooms.get_mut(code) {
        room.players.retain(|player| player.id != player_id);

        if room.players.is_empty() {
            session_id_to_remove = room.game_session_id.clone();
            should_remove_room = true;
        } else {
            if room.host_id == player_id
                && let Some(next_host_id) = next_host_id(&room.players)
            {
                room.host_id = next_host_id.clone();
            }

            if matches!(room.status, RoomStatus::Playing) {
                should_remove_session = true;
                session_id_to_remove = room.game_session_id.clone();
                reset_room_after_game(room);
            } else {
                for player in &mut room.players {
                    if player.id == room.host_id {
                        player.is_ready = true;
                    }
                }
            }
        }
    }

    if should_remove_room {
        guard.rooms.remove(code);
    }
    if should_remove_session && let Some(session_id) = session_id_to_remove {
        guard.sessions.remove(&session_id);
    }
}

#[allow(dead_code)]
fn release_room_after_game(guard: &mut RoomRepository, room_code: &str) {
    let session_id = guard
        .rooms
        .get(room_code)
        .and_then(|room| room.game_session_id.clone());

    if let Some(room) = guard.rooms.get_mut(room_code) {
        reset_room_after_game(room);
    }

    if let Some(session_id) = session_id {
        guard.sessions.remove(&session_id);
    }
}

fn reset_room_after_game(room: &mut Room) {
    room.status = RoomStatus::Waiting;
    room.game_session_id = None;
    for player in &mut room.players {
        player.is_ready = player.id == room.host_id;
    }
}

fn public_room(room: &Room) -> Room {
    let mut room = room.clone();
    room.has_password = room.password.is_some();
    room.password = None;
    room
}

async fn player_from_claims(
    user_repo: &UserRepository,
    claims: &TokenClaims,
    is_ready: bool,
) -> Result<Player, AppError> {
    let user = user_repo
        .find_by_id(&claims.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Player {
        id: user.id.to_string(),
        username: user.name,
        avatar: user.avatar,
        is_ready,
        joined_at: Some(now_millis()),
    })
}

async fn refresh_room_player_profiles(user_repo: &UserRepository, room: &mut Room) {
    for player in &mut room.players {
        let Ok(user_uuid) = uuid::Uuid::parse_str(&player.id) else {
            continue;
        };
        let Ok(Some(user)) = user_repo.find_by_id(&UserId(user_uuid)).await else {
            continue;
        };
        player.username = user.name;
        player.avatar = user.avatar;
    }
}

fn build_game_snapshot(
    session: &GameSession,
    rule_id: &str,
    round_time: u32,
    viewer_id: &str,
    room: Option<&Room>,
    assets: RuleAssets,
) -> GameSnapshotView {
    let current_player_id = session
        .pending_action
        .as_ref()
        .map(|action| action.player_id.clone())
        .unwrap_or_else(|| {
            let index = session
                .table
                .get("player_index")
                .copied()
                .unwrap_or_default();
            session
                .players
                .get(index.max(0) as usize)
                .map(|player| player.id.clone())
                .unwrap_or_default()
        });
    let played_cards = if session.last_action_skipped {
        Vec::new()
    } else {
        session
            .last_successful_play
            .as_ref()
            .map(|play| play.cards.iter().map(to_card_view).collect())
            .unwrap_or_default()
    };
    let hand_cards = session
        .hands
        .get(viewer_id)
        .map(|cards| cards.iter().map(to_card_view).collect())
        .unwrap_or_default();
    let winner_ids = single_winner_ids(session);
    let pending_action = session
        .pending_action
        .as_ref()
        .map(|action| PendingActionView {
            action_id: action.id.clone(),
            action_type: if action.component_type == 22 {
                "choose_option".to_string()
            } else {
                "play_cards".to_string()
            },
            player_id: action.player_id.clone(),
            timer: action.timer,
            deadline_at: Some(now_millis() + action.timer as i64 * 1000),
            can_skip: session.last_successful_play.is_some(),
            options: action.options.clone(),
        });

    GameSnapshotView {
        session_id: session.id.clone(),
        room_code: session.room_code.clone(),
        rule_id: rule_id.to_string(),
        status: if session.status == "finished" {
            "finished".to_string()
        } else if session.active_flow == "end" {
            "settling".to_string()
        } else {
            "playing".to_string()
        },
        current_player_id,
        round_time,
        deadline_at: pending_action
            .as_ref()
            .and_then(|action| action.deadline_at),
        players: session
            .players
            .iter()
            .map(|player| {
                let room_player = room.and_then(|room| {
                    room.players
                        .iter()
                        .find(|room_player| room_player.id == player.id)
                });
                GamePlayerView {
                    id: player.id.clone(),
                    username: room_player
                        .map(|player| player.username.clone())
                        .unwrap_or_else(|| format!("玩家{}", player.runtime_index + 1)),
                    avatar: room_player
                        .map(|player| player.avatar.clone())
                        .unwrap_or_default(),
                    card_count: session
                        .hands
                        .get(&player.id)
                        .map(Vec::len)
                        .unwrap_or_default(),
                    public_properties: player.properties.clone(),
                    online: true,
                }
            })
            .collect(),
        table: GameTableView {
            played_cards,
            public_properties: session.table.clone(),
        },
        hand_cards,
        pending_action,
        last_action: session.last_action_player_id.as_ref().map(|player_id| {
            let skipped = session.last_action_skipped;
            let cards = session
                .last_action_cards
                .iter()
                .map(to_card_view)
                .collect::<Vec<_>>();
            GameActionRecordView {
                player_id: player_id.clone(),
                action: if skipped { "skip" } else { "play_cards" }.to_string(),
                message: if skipped {
                    "Player skipped".to_string()
                } else {
                    format!("Played {} card(s)", cards.len())
                },
                cards,
                turn: session.execution_log.len() as u32,
            }
        }),
        winner_ids,
        assets,
    }
}

async fn load_rule_assets(rule_store: &RuleStore, rule_id: &str) -> RuleAssets {
    let rule_guard = rule_store.read().await;
    rule_guard
        .published
        .get(rule_id)
        .map(|published| published.runtime.design.assets.clone())
        .unwrap_or_default()
}

fn to_card_view(card: &GameCard) -> GameCardView {
    let point = card
        .properties
        .get("point")
        .copied()
        .unwrap_or_else(|| card.properties.get("点数").copied().unwrap_or_default());
    let suit = card
        .properties
        .get("suit")
        .copied()
        .unwrap_or_else(|| card.properties.get("花色").copied().unwrap_or_default());

    GameCardView {
        id: card.id.clone(),
        properties: card.properties.clone(),
        display: GameCardDisplay {
            rank: rank_display(point),
            suit: suit_display(suit),
        },
    }
}

fn single_winner_ids(session: &GameSession) -> Vec<String> {
    session
        .players
        .iter()
        .filter_map(|player| {
            session
                .settlement_results
                .get(&player.id)
                .copied()
                .map(|result| (player.id.clone(), result))
        })
        .max_by_key(|(_, result)| *result)
        .and_then(|(player_id, result)| (result > 0).then_some(player_id))
        .into_iter()
        .collect()
}

fn rank_display(point: i64) -> String {
    match point {
        1 | 14 => "A".to_string(),
        11 => "J".to_string(),
        12 => "Q".to_string(),
        13 => "K".to_string(),
        value if value > 0 => value.to_string(),
        _ => "?".to_string(),
    }
}

fn suit_display(suit: i64) -> String {
    match suit {
        0 => "S",
        1 => "H",
        2 => "C",
        3 => "D",
        _ => "?",
    }
    .to_string()
}

fn next_host_id(players: &[Player]) -> Option<String> {
    players
        .iter()
        .min_by_key(|player| player.joined_at.unwrap_or_default())
        .map(|player| player.id.clone())
}

fn normalize_password(password: Option<String>) -> Option<String> {
    password.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn generate_room_code(rooms: &HashMap<String, Room>) -> String {
    loop {
        let candidate = uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(6)
            .collect::<String>()
            .to_uppercase();
        if !rooms.contains_key(&candidate) {
            return candidate;
        }
    }
}

fn now_millis() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}
