use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    domain::game::GameSession,
    domain::room::{
        GameRuleOption, Player, Room, RoomRuleResponse, RoomStatus, RuleCatalogEntry,
        default_rule_catalog,
    },
    infrastructure::user::UserRepository,
    interface::{auth::TokenClaims, game, user::ApiResponse},
    state::JwtSecret,
};

const PLAYER_ID_HEADER: &str = "x-player-id";
const PLAYER_NAME_HEADER: &str = "x-player-name";
const PLAYER_AVATAR_HEADER: &str = "x-player-avatar";

type SharedRoomStore = Arc<RwLock<HashMap<String, Room>>>;
type SharedRuleCatalog = Arc<HashMap<String, RuleCatalogEntry>>;
type SharedGameStore = Arc<RwLock<HashMap<String, GameSession>>>;

#[derive(Debug)]
pub enum RoomApiError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
    domain::rule_engine::{GameSession, PlayerActionInput, RuleEngine},
    error::AppError,
    interface::{auth::TokenClaims, rule::ApiResponse},
    state::{RoomStore, RuleStore},
};

#[derive(Debug, Default)]
pub struct RoomRepository {
    pub rooms: HashMap<String, Room>,
    pub player_rooms: HashMap<String, String>,
    pub sessions: HashMap<String, GameSession>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Room {
    pub id: String,
    pub code: String,
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
    #[serde(rename = "roundTime")]
    pub round_time: u16,
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    #[serde(rename = "ruleName")]
    pub rule_name: String,
    pub password: Option<String>,
    pub players: Vec<Player>,
    pub status: RoomStatus,
    #[serde(rename = "gameSessionId", skip_serializing_if = "Option::is_none")]
    pub game_session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentParticipant {
    pub(crate) room_player_id: String,
    pub(crate) username: String,
    pub(crate) avatar: String,
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomStatus {
    Waiting,
    Playing,
    Finished,
}

#[derive(Debug, Clone, Serialize)]
pub struct Player {
    pub id: String,
    pub username: String,
    pub avatar: String,
    #[serde(rename = "isReady")]
    pub is_ready: bool,
    #[serde(rename = "joinedAt")]
    pub joined_at: i64,
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

#[derive(Debug, Serialize)]
pub struct RoomRuleResponse {
    pub room_id: String,
    pub rule: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct CurrentGameQuery {
    #[serde(rename = "roomCode")]
    pub room_code: Option<String>,
}

pub(crate) async fn resolve_current_participant(
    headers: &HeaderMap,
    jwt_secret: &JwtSecret,
    user_repo: &Arc<UserRepository>,
) -> Result<CurrentParticipant, RoomApiError> {
    if let Some(TokenClaims { user_id, .. }) = parse_claims_from_headers(headers, &jwt_secret.0) {
        let user = user_repo
            .find_by_id(&user_id)
            .await
            .map_err(|error| RoomApiError::Internal(error.to_string()))?
            .ok_or_else(|| RoomApiError::Unauthorized("当前登录用户不存在".to_string()))?;

    let player_id = user_id.to_string();
    let mut guard = room_store.write().await;
    if let Some(old_code) = guard.player_rooms.remove(&player_id) {
        remove_player_from_room(&mut guard.rooms, &old_code, &player_id);
    }

    let code = generate_room_code(&guard.rooms);
    let room = Room {
        id: uuid::Uuid::new_v4().to_string(),
        code: code.clone(),
        host_id: player_id.clone(),
        player_count: published_rule.player_count,
        round_time: payload.round_time,
        rule_id: published_rule.id,
        rule_name: published_rule.name,
        password: payload.password.filter(|value| !value.trim().is_empty()),
        players: vec![Player {
            id: player_id.clone(),
            username: format!("玩家{}", &player_id[..8.min(player_id.len())]),
            avatar: String::new(),
            is_ready: true,
            joined_at: now_millis(),
        }],
        status: RoomStatus::Waiting,
        game_session_id: None,
    };

    guard.player_rooms.insert(player_id, code.clone());
    guard.rooms.insert(code, room.clone());

    Ok(Json(ApiResponse::success(room)))
}

pub async fn join_room(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_store): State<RoomStore>,
    Json(payload): Json<JoinRoomRequest>,
) -> Result<Json<ApiResponse<Room>>, AppError> {
    let player_id = user_id.to_string();
    let mut guard = room_store.write().await;
    let room = {
        let room = guard
            .rooms
            .get_mut(&payload.code)
            .ok_or(AppError::NotFound)?;

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
            && room.players.len() >= room.player_count as usize
        {
            return Err(AppError::InvalidInput("房间已满".to_string()));
        }

        if !room.players.iter().any(|player| player.id == player_id) {
            room.players.push(Player {
                id: player_id.clone(),
                username: format!("玩家{}", &player_id[..8.min(player_id.len())]),
                avatar: String::new(),
                is_ready: false,
                joined_at: now_millis(),
            });
        }

        room.clone()
    };

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
            .get(&code)
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
    let guard = room_store.read().await;
    let room = if let Some(code) = query.code {
        guard.rooms.get(&code).cloned()
    } else {
        guard
            .player_rooms
            .get(&user_id.to_string())
            .and_then(|code| guard.rooms.get(code))
            .cloned()
    };

    Ok(Json(ApiResponse::success(room)))
}

pub async fn set_ready(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_store): State<RoomStore>,
    Json(payload): Json<ReadyRequest>,
) -> Result<Json<ApiResponse<Room>>, AppError> {
    let player_id = user_id.to_string();
    let mut guard = room_store.write().await;
    let code = guard
        .player_rooms
        .get(&player_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    let room = guard.rooms.get_mut(&code).ok_or(AppError::NotFound)?;
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
    player.is_ready = payload.is_ready;

    Ok(Json(ApiResponse::success(room.clone())))
}

pub async fn start_game(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
) -> Result<Json<ApiResponse<Room>>, AppError> {
    let player_id = user_id.to_string();
    let mut room_guard = room_store.write().await;
    let code = room_guard
        .player_rooms
        .get(&player_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    let room = room_guard.rooms.get_mut(&code).ok_or(AppError::NotFound)?;

    if room.host_id != player_id {
        return Err(AppError::Unauthorized("只有房主可以开始游戏".to_string()));
    }
    if room.players.len() != room.player_count as usize
        || !room.players.iter().all(|player| player.is_ready)
    {
        return Err(AppError::InvalidInput(
            "房间必须满员且所有玩家已准备".to_string(),
        ));
    }

    let runtime_rule = {
        let rule_guard = rule_store.read().await;
        let published = rule_guard
            .published
            .get(&room.rule_id)
            .ok_or(AppError::NotFound)?;
        // 开局时按已发布 JSON 再解析一次，保证规则引擎运行态来自当前规则内容。
        RuleEngine::parse(
            published.name.clone(),
            published.player_count,
            published.description.clone(),
            published.design.clone(),
        )?
    };
    let player_ids = room
        .players
        .iter()
        .map(|player| player.id.clone())
        .collect();
    let session = RuleEngine::start_session(room.code.clone(), &runtime_rule, player_ids)?;
    let session_id = session.id.clone();
    let room = {
        room.status = RoomStatus::Playing;
        room.game_session_id = Some(session_id.clone());
        room.clone()
    };
    room_guard.sessions.insert(session_id, session);

    Ok(Json(ApiResponse::success(room)))
}

pub async fn current_game(
    State(room_store): State<RoomStore>,
    Query(query): Query<CurrentGameQuery>,
) -> Result<Json<ApiResponse<GameSession>>, AppError> {
    let guard = room_store.read().await;
    let session_id = query
        .room_code
        .as_deref()
        .and_then(|code| guard.rooms.get(code))
        .and_then(|room| room.game_session_id.as_ref())
        .ok_or(AppError::NotFound)?;
    let session = guard
        .sessions
        .get(session_id)
        .cloned()
        .ok_or(AppError::NotFound)?;

    Ok(Json(ApiResponse::success(session)))
}

pub async fn get_game(
    State(room_store): State<RoomStore>,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<GameSession>>, AppError> {
    let guard = room_store.read().await;
    let session = guard
        .sessions
        .get(&session_id)
        .cloned()
        .ok_or(AppError::NotFound)?;

    Ok(Json(ApiResponse::success(session)))
}

pub async fn play_cards(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    Path((session_id, action_id)): Path<(String, String)>,
    Json(payload): Json<PlayerActionInput>,
) -> Result<Json<ApiResponse<GameSession>>, AppError> {
    submit_game_action(
        user_id.to_string(),
        rule_store,
        room_store,
        session_id,
        action_id,
        payload,
    )
    .await
}

pub async fn choose_action(
    TokenClaims { user_id, .. }: TokenClaims,
    State(rule_store): State<RuleStore>,
    State(room_store): State<RoomStore>,
    Path((session_id, action_id)): Path<(String, String)>,
    Json(payload): Json<PlayerActionInput>,
) -> Result<Json<ApiResponse<GameSession>>, AppError> {
    submit_game_action(
        user_id.to_string(),
        rule_store,
        room_store,
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
    Path((session_id, action_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<GameSession>>, AppError> {
    submit_game_action(
        user_id.to_string(),
        rule_store,
        room_store,
        session_id,
        action_id,
        PlayerActionInput {
            cards: Vec::new(),
            choice: None,
        },
    )
    .await
}

async fn submit_game_action(
    player_id: String,
    rule_store: RuleStore,
    room_store: RoomStore,
    session_id: String,
    action_id: String,
    payload: PlayerActionInput,
) -> Result<Json<ApiResponse<GameSession>>, AppError> {
    let mut room_guard = room_store.write().await;
    let room_code = room_guard
        .sessions
        .get(&session_id)
        .map(|session| session.room_code.clone())
        .ok_or(AppError::NotFound)?;
    let room = room_guard.rooms.get(&room_code).ok_or(AppError::NotFound)?;
    let runtime_rule = {
        let rule_guard = rule_store.read().await;
        let published = rule_guard
            .published
            .get(&room.rule_id)
            .ok_or(AppError::NotFound)?;
        // 动作提交后需要同一份规则运行态继续推进流程。
        RuleEngine::parse(
            published.name.clone(),
            published.player_count,
            published.description.clone(),
            published.design.clone(),
        )?
    };
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
    Ok(Json(ApiResponse::success(session.clone())))
}

pub async fn leave_room(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_store): State<RoomStore>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let player_id = user_id.to_string();
    let mut guard = room_store.write().await;
    if let Some(code) = guard.player_rooms.remove(&player_id) {
        remove_player_from_room(&mut guard.rooms, &code, &player_id);
    }

    Ok(Json(ApiResponse {
        success: true,
        data: None,
        message: Some("已离开房间".to_string()),
    }))
}

pub async fn start_game(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(rooms): State<SharedRoomStore>,
    State(games): State<SharedGameStore>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Option<Room>>>, RoomApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let mut guard = rooms.write().await;
    let room = find_room_by_player_mut(&mut guard, &participant.room_player_id)
        .ok_or_else(|| RoomApiError::NotFound("房间不存在".to_string()))?;

    if room.host_id != participant.room_player_id {
        return Err(RoomApiError::Forbidden("只有房主可以开始游戏".to_string()));
    }

    if !is_room_ready_to_start(room) {
        return Err(RoomApiError::Conflict(
            "房间人数未满或仍有玩家未准备".to_string(),
        ));
    }

    room.status = RoomStatus::Playing;
    let session = game::create_game_session(room);
    game::store_game_session(&games, session).await;
    Ok(Json(ApiResponse::success_with_optional_data(Some(Some(
        sanitize_room(room),
    )))))
}

pub async fn leave_room(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(rooms): State<SharedRoomStore>,
    State(games): State<SharedGameStore>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<()>>, RoomApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let mut guard = rooms.write().await;
    let room_code = guard
        .iter()
        .find(|(_, room)| {
            room.players
                .iter()
                .any(|player| player.id == participant.room_player_id)
        })
        .map(|(code, _)| code.clone());

    let Some(room_code) = room_code else {
        return Ok(Json(ApiResponse::success_without_data(None)));
    };

    let should_remove_room = {
        let room = guard
            .get_mut(&room_code)
            .ok_or_else(|| RoomApiError::NotFound("房间不存在".to_string()))?;

        room.players
            .retain(|player| player.id != participant.room_player_id);

        if room.players.is_empty() {
            true
        } else {
            if room.host_id == participant.room_player_id
                && let Some(new_host_id) = next_host_id(&room.players)
            {
                room.host_id = new_host_id.clone();
                if let Some(new_host) = room
                    .players
                    .iter_mut()
                    .find(|player| player.id == new_host_id)
                {
                    new_host.is_ready = true;
                }
            }

            if room.status == RoomStatus::Playing {
                room.status = RoomStatus::Waiting;
                for player in &mut room.players {
                    player.is_ready = player.id == room.host_id;
                }
            }

            false
        }
    }
}

fn remove_player_from_room(rooms: &mut HashMap<String, Room>, code: &str, player_id: &str) {
    let Some(room) = rooms.get_mut(code) else {
        return;
    };

    room.players.retain(|player| player.id != player_id);
    if room.players.is_empty() {
        rooms.remove(code);
        return;
    }

    game::end_game_for_room(&games, &room_code).await;

    Ok(Json(ApiResponse::success_without_data(None)))
}

fn now_millis() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}
