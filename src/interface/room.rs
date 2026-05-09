use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

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
}

impl IntoResponse for RoomApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        };

        (status, Json(ApiResponse::<()>::failure(message))).into_response()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentParticipant {
    pub(crate) room_player_id: String,
    pub(crate) username: String,
    pub(crate) avatar: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoomRequest {
    pub rule_id: String,
    pub round_time: u32,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JoinRoomRequest {
    pub code: String,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRoomPasswordQuery {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct CurrentRoomQuery {
    pub code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RoomRuleQuery {
    pub room_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyRequest {
    pub is_ready: bool,
}

#[derive(Debug, Serialize)]
pub struct CheckRoomPasswordResponse {
    pub success: bool,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
}

fn unix_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_claims_from_headers(headers: &HeaderMap, jwt_secret: &[u8]) -> Option<TokenClaims> {
    let jar = CookieJar::from_headers(headers);
    let token = jar.get("token")?.value().to_string();

    jsonwebtoken::decode::<TokenClaims>(
        &token,
        &DecodingKey::from_secret(jwt_secret),
        &Validation::default(),
    )
    .ok()
    .map(|token_data| token_data.claims)
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

        return Ok(CurrentParticipant {
            room_player_id: user_id.to_string(),
            username: user.name,
            avatar: String::new(),
        });
    }

    let player_id = trimmed_optional(
        headers
            .get(PLAYER_ID_HEADER)
            .and_then(|value| value.to_str().ok()),
    )
    .ok_or_else(|| RoomApiError::Unauthorized("缺少房间玩家身份信息".to_string()))?;

    let username = trimmed_optional(
        headers
            .get(PLAYER_NAME_HEADER)
            .and_then(|value| value.to_str().ok()),
    )
    .and_then(|value| {
        urlencoding::decode(&value)
            .ok()
            .map(|decoded| decoded.into_owned())
    })
    .unwrap_or_else(|| format!("guest-{}", &player_id.chars().take(6).collect::<String>()));

    let avatar = headers
        .get(PLAYER_AVATAR_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            urlencoding::decode(value)
                .ok()
                .map(|decoded| decoded.into_owned())
        })
        .unwrap_or_default();

    Ok(CurrentParticipant {
        room_player_id: player_id,
        username,
        avatar,
    })
}

fn sanitize_room(room: &Room) -> Room {
    let mut sanitized = room.clone();
    sanitized.password = None;
    sanitized
}

fn generate_room_code(existing_rooms: &HashMap<String, Room>) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

    loop {
        let raw = *Uuid::new_v4().as_bytes();
        let mut code = String::with_capacity(6);

        for byte in raw.iter().take(6) {
            code.push(ALPHABET[usize::from(*byte) % ALPHABET.len()] as char);
        }

        if !existing_rooms.contains_key(&code) {
            return code;
        }
    }
}

fn find_room_by_player<'a>(rooms: &'a HashMap<String, Room>, player_id: &str) -> Option<&'a Room> {
    rooms
        .values()
        .find(|room| room.players.iter().any(|player| player.id == player_id))
}

fn find_room_by_player_mut<'a>(
    rooms: &'a mut HashMap<String, Room>,
    player_id: &str,
) -> Option<&'a mut Room> {
    rooms
        .values_mut()
        .find(|room| room.players.iter().any(|player| player.id == player_id))
}

fn is_room_ready_to_start(room: &Room) -> bool {
    room.status == RoomStatus::Waiting
        && room.players.len() == room.player_count
        && room.players.iter().all(|player| player.is_ready)
}

fn next_host_id(players: &[Player]) -> Option<String> {
    players
        .iter()
        .min_by_key(|player| player.joined_at.unwrap_or(0))
        .map(|player| player.id.clone())
}

fn read_rule_for_room(
    room_id: Option<&str>,
    rooms: &HashMap<String, Room>,
    rules: &HashMap<String, RuleCatalogEntry>,
) -> Option<RoomRuleResponse> {
    let selected_room = room_id.and_then(|room_id| {
        rooms
            .values()
            .find(|room| room.id == room_id || room.code == room_id)
    });

    let room = selected_room.or_else(|| rooms.values().next())?;
    let definition = rules
        .get(&room.rule_id)
        .or_else(|| rules.get("classic"))
        .map(|entry| entry.definition.clone())?;

    Some(RoomRuleResponse {
        room_id: room.id.clone(),
        rule: definition,
    })
}

pub async fn get_rule_options(
    State(rules): State<SharedRuleCatalog>,
) -> Json<ApiResponse<Vec<GameRuleOption>>> {
    let mut data = rules
        .values()
        .map(|entry| entry.option.clone())
        .collect::<Vec<_>>();
    data.sort_by(|left, right| left.id.cmp(&right.id));
    Json(ApiResponse::success(data))
}

pub async fn create_room(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(rooms): State<SharedRoomStore>,
    State(rules): State<SharedRuleCatalog>,
    headers: HeaderMap,
    Json(payload): Json<CreateRoomRequest>,
) -> Result<Json<ApiResponse<Room>>, RoomApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let rule_id = payload.rule_id.trim();
    let selected_rule = rules
        .get(rule_id)
        .ok_or_else(|| RoomApiError::BadRequest("无效的房间规则".to_string()))?;

    if payload.round_time == 0 {
        return Err(RoomApiError::BadRequest("回合时长必须大于 0".to_string()));
    }

    let mut guard = rooms.write().await;
    if find_room_by_player(&guard, &participant.room_player_id).is_some() {
        return Err(RoomApiError::Conflict("当前玩家已经在房间中".to_string()));
    }

    let code = generate_room_code(&guard);
    let room = Room {
        id: format!("room_{}", Uuid::new_v4().simple()),
        code: code.clone(),
        host_id: participant.room_player_id.clone(),
        player_count: selected_rule.option.player_count,
        round_time: payload.round_time,
        rule_id: selected_rule.option.id.clone(),
        rule_name: selected_rule.option.name.clone(),
        password: trimmed_optional(payload.password.as_deref()),
        players: vec![Player {
            id: participant.room_player_id,
            username: participant.username,
            avatar: participant.avatar,
            is_ready: true,
            joined_at: Some(unix_timestamp_millis()),
        }],
        status: RoomStatus::Waiting,
    };

    let response = sanitize_room(&room);
    guard.insert(code, room);
    Ok(Json(ApiResponse::success(response)))
}

pub async fn check_room_password(
    State(rooms): State<SharedRoomStore>,
    Query(query): Query<CheckRoomPasswordQuery>,
) -> Result<Json<CheckRoomPasswordResponse>, RoomApiError> {
    let code = query.code.trim().to_uppercase();
    let guard = rooms.read().await;
    let room = guard
        .get(&code)
        .ok_or_else(|| RoomApiError::NotFound("房间不存在".to_string()))?;

    Ok(Json(CheckRoomPasswordResponse {
        success: true,
        has_password: room
            .password
            .as_ref()
            .is_some_and(|password| !password.is_empty()),
    }))
}

pub async fn join_room(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(rooms): State<SharedRoomStore>,
    headers: HeaderMap,
    Json(payload): Json<JoinRoomRequest>,
) -> Result<Json<ApiResponse<Room>>, RoomApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let room_code = payload.code.trim().to_uppercase();

    if room_code.is_empty() {
        return Err(RoomApiError::BadRequest("房间号不能为空".to_string()));
    }

    let mut guard = rooms.write().await;
    if let Some(current_room) = find_room_by_player(&guard, &participant.room_player_id)
        && current_room.code != room_code
    {
        return Err(RoomApiError::Conflict(
            "当前玩家已经在其他房间中".to_string(),
        ));
    }

    let room = guard
        .get_mut(&room_code)
        .ok_or_else(|| RoomApiError::NotFound("房间不存在".to_string()))?;

    if room.status != RoomStatus::Waiting {
        return Err(RoomApiError::Conflict("该房间已经开始游戏".to_string()));
    }

    if room.password != trimmed_optional(payload.password.as_deref()) {
        return Err(RoomApiError::Forbidden("房间密码错误".to_string()));
    }

    if let Some(existing_player) = room
        .players
        .iter_mut()
        .find(|player| player.id == participant.room_player_id)
    {
        existing_player.username = participant.username;
        existing_player.avatar = participant.avatar;
        return Ok(Json(ApiResponse::success(sanitize_room(room))));
    }

    if room.players.len() >= room.player_count {
        return Err(RoomApiError::Conflict("房间人数已满".to_string()));
    }

    room.players.push(Player {
        id: participant.room_player_id,
        username: participant.username,
        avatar: participant.avatar,
        is_ready: false,
        joined_at: Some(unix_timestamp_millis()),
    });

    Ok(Json(ApiResponse::success(sanitize_room(room))))
}

pub async fn get_current_room(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(rooms): State<SharedRoomStore>,
    headers: HeaderMap,
    Query(query): Query<CurrentRoomQuery>,
) -> Result<Json<ApiResponse<Option<Room>>>, RoomApiError> {
    let guard = rooms.read().await;

    if let Some(code) = trimmed_optional(query.code.as_deref()) {
        let room = guard
            .get(&code.to_uppercase())
            .map(sanitize_room)
            .ok_or_else(|| RoomApiError::NotFound("房间不存在".to_string()))?;

        return Ok(Json(ApiResponse::success_with_optional_data(Some(Some(
            room,
        )))));
    }

    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let room = find_room_by_player(&guard, &participant.room_player_id).map(sanitize_room);
    Ok(Json(ApiResponse::success_with_optional_data(Some(room))))
}

pub async fn get_room_rule(
    State(rooms): State<SharedRoomStore>,
    State(rules): State<SharedRuleCatalog>,
    Query(query): Query<RoomRuleQuery>,
) -> Result<Json<RoomRuleResponse>, RoomApiError> {
    let guard = rooms.read().await;
    let response = read_rule_for_room(query.room_id.as_deref(), &guard, &rules)
        .ok_or_else(|| RoomApiError::NotFound("未找到房间规则".to_string()))?;
    Ok(Json(response))
}

pub async fn set_ready(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(rooms): State<SharedRoomStore>,
    headers: HeaderMap,
    Json(payload): Json<ReadyRequest>,
) -> Result<Json<ApiResponse<Option<Room>>>, RoomApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let mut guard = rooms.write().await;
    let room = find_room_by_player_mut(&mut guard, &participant.room_player_id)
        .ok_or_else(|| RoomApiError::NotFound("房间不存在".to_string()))?;

    if room.status != RoomStatus::Waiting {
        return Err(RoomApiError::Conflict("房间已开始游戏".to_string()));
    }

    let player = room
        .players
        .iter_mut()
        .find(|player| player.id == participant.room_player_id)
        .ok_or_else(|| RoomApiError::NotFound("当前玩家不在房间中".to_string()))?;

    if room.host_id == participant.room_player_id && !payload.is_ready {
        player.is_ready = true;
    } else {
        player.is_ready = payload.is_ready;
    }

    Ok(Json(ApiResponse::success_with_optional_data(Some(Some(
        sanitize_room(room),
    )))))
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
    };

    if should_remove_room {
        guard.remove(&room_code);
    }

    game::end_game_for_room(&games, &room_code).await;

    Ok(Json(ApiResponse::success_without_data(None)))
}

pub fn build_default_room_state() -> (SharedRoomStore, SharedRuleCatalog) {
    (
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(default_rule_catalog()),
    )
}
