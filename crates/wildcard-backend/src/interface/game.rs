use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::{
        game::{
            GameActionRecord, GameCard, GameCardDisplay, GameCardProperties, GamePlayerState,
            GamePlayerView, GameSession, GameSnapshot, GameStatus, GameTableView, PendingAction,
            PendingActionType, PlayCardsRequest,
        },
        room::{Player, Room, RoomStatus},
    },
    infrastructure::user::UserRepository,
    interface::{
        room::{RoomApiError, resolve_current_participant},
        user::ApiResponse,
    },
    state::JwtSecret,
};

#[derive(Debug)]
pub enum GameApiError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl IntoResponse for GameApiError {
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

impl From<RoomApiError> for GameApiError {
    fn from(value: RoomApiError) -> Self {
        match value {
            RoomApiError::BadRequest(message) => Self::BadRequest(message),
            RoomApiError::Unauthorized(message) => Self::Unauthorized(message),
            RoomApiError::Forbidden(message) => Self::Forbidden(message),
            RoomApiError::NotFound(message) => Self::NotFound(message),
            RoomApiError::Conflict(message) => Self::Conflict(message),
            RoomApiError::Internal(message) => Self::Internal(message),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CurrentGameQuery {
    #[serde(rename = "roomCode")]
    pub room_code: String,
}

const SUITS: [&str; 4] = ["♠", "♥", "♣", "♦"];
const RANKS: [&str; 13] = [
    "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A", "2",
];

fn build_deck() -> Vec<GameCard> {
    let mut deck = Vec::with_capacity(SUITS.len() * RANKS.len());

    for (suit_index, suit) in SUITS.iter().enumerate() {
        for (point_index, rank) in RANKS.iter().enumerate() {
            deck.push(GameCard {
                id: format!("card-{}-{}", suit_index, point_index),
                properties: GameCardProperties {
                    point: (point_index + 3) as u8,
                    suit: suit_index as u8,
                },
                display: GameCardDisplay {
                    rank: (*rank).to_string(),
                    suit: (*suit).to_string(),
                },
            });
        }
    }

    deck
}

fn cards_per_player(player_count: usize) -> usize {
    if player_count == 0 {
        return 0;
    }

    build_deck().len() / player_count
}

fn create_player_states(room_players: &[Player]) -> Vec<GamePlayerState> {
    let deck = build_deck();
    let hand_size = cards_per_player(room_players.len());

    room_players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            let start = index * hand_size;
            let end = start + hand_size;
            let hand_cards = deck[start..end].to_vec();

            GamePlayerState {
                id: player.id.clone(),
                username: player.username.clone(),
                avatar: player.avatar.clone(),
                hand_cards,
                finished_at_turn: None,
            }
        })
        .collect()
}

fn build_snapshot(session: &GameSession, player_id: &str) -> GameSnapshot {
    let players = session
        .players
        .iter()
        .map(|player| GamePlayerView {
            id: player.id.clone(),
            username: player.username.clone(),
            avatar: player.avatar.clone(),
            card_count: player.hand_cards.len(),
            online: true,
            finished: player.finished_at_turn.is_some(),
        })
        .collect::<Vec<_>>();

    let hand_cards = session
        .players
        .iter()
        .find(|player| player.id == player_id)
        .map(|player| player.hand_cards.clone())
        .unwrap_or_default();

    let pending_action = matches!(session.status, GameStatus::Playing).then(|| PendingAction {
        action_id: format!("action-{}", session.turn),
        player_id: session.current_player_id.clone(),
        action_type: PendingActionType::PlayCards,
        can_skip: !session.table.played_cards.is_empty(),
    });

    GameSnapshot {
        session_id: session.session_id.clone(),
        room_code: session.room_code.clone(),
        rule_id: session.rule_id.clone(),
        status: session.status.clone(),
        current_player_id: session.current_player_id.clone(),
        round_time: session.round_time,
        deadline_at: None,
        players,
        table: session.table.clone(),
        hand_cards,
        pending_action,
        last_action: session.last_action.clone(),
        winner_ids: session.winner_ids.clone(),
    }
}

fn rotate_to_next_player(session: &mut GameSession) {
    let active_players = session
        .players
        .iter()
        .filter(|player| player.finished_at_turn.is_none())
        .map(|player| player.id.clone())
        .collect::<Vec<_>>();

    if active_players.len() <= 1 {
        session.status = GameStatus::Finished;
        session.winner_ids = active_players;
        return;
    }

    let current_index = active_players
        .iter()
        .position(|player_id| player_id == &session.current_player_id)
        .unwrap_or(0);
    let next_index = (current_index + 1) % active_players.len();
    session.current_player_id = active_players[next_index].clone();
    session.turn += 1;
}

fn reset_room_after_game(room: &mut Room) {
    room.status = RoomStatus::Waiting;
    for player in &mut room.players {
        player.is_ready = player.id == room.host_id;
    }
}

async fn release_room_after_game(rooms: &SharedRoomStore, room_code: &str) {
    let mut room_guard = rooms.write().await;
    if let Some(room) = room_guard.values_mut().find(|room| room.code == room_code) {
        reset_room_after_game(room);
    }
}

async fn finalize_finished_game(games: &SharedGameStore, rooms: &SharedRoomStore, room_code: &str) {
    release_room_after_game(rooms, room_code).await;
    end_game_for_room(games, room_code).await;
}

pub fn create_game_session(room: &Room) -> GameSession {
    let players = create_player_states(&room.players);
    let first_player_id = players
        .first()
        .map(|player| player.id.clone())
        .unwrap_or_default();

    GameSession {
        session_id: format!("game_{}", Uuid::new_v4().simple()),
        room_code: room.code.clone(),
        room_id: room.id.clone(),
        rule_id: room.rule_id.clone(),
        status: GameStatus::Playing,
        current_player_id: first_player_id,
        round_time: room.round_time,
        turn: 1,
        players,
        table: GameTableView {
            played_cards: Vec::new(),
            pass_streak: 0,
            last_played_by: None,
        },
        last_action: None,
        winner_ids: Vec::new(),
    }
}

pub async fn store_game_session(games: &SharedGameStore, session: GameSession) {
    let mut guard = games.write().await;
    let existing_ids = guard
        .values()
        .filter(|existing| existing.room_code == session.room_code)
        .map(|existing| existing.session_id.clone())
        .collect::<Vec<_>>();

    for session_id in existing_ids {
        guard.remove(&session_id);
    }

    guard.insert(session.session_id.clone(), session);
}

pub async fn get_current_game(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(games): State<SharedGameStore>,
    headers: HeaderMap,
    Query(query): Query<CurrentGameQuery>,
) -> Result<Json<ApiResponse<GameSnapshot>>, GameApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let guard = games.read().await;
    let session = guard
        .values()
        .find(|session| session.room_code == query.room_code.to_uppercase())
        .ok_or_else(|| GameApiError::NotFound("当前房间没有进行中的对局".to_string()))?;

    Ok(Json(ApiResponse::success(build_snapshot(
        session,
        &participant.room_player_id,
    ))))
}

pub async fn get_game_by_session(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(games): State<SharedGameStore>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<GameSnapshot>>, GameApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let guard = games.read().await;
    let session = guard
        .get(&session_id)
        .ok_or_else(|| GameApiError::NotFound("对局不存在".to_string()))?;

    Ok(Json(ApiResponse::success(build_snapshot(
        session,
        &participant.room_player_id,
    ))))
}

pub async fn play_cards(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(games): State<SharedGameStore>,
    State(rooms): State<SharedRoomStore>,
    headers: HeaderMap,
    Path((session_id, _action_id)): Path<(String, String)>,
    Json(payload): Json<PlayCardsRequest>,
) -> Result<Json<ApiResponse<GameSnapshot>>, GameApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let (snapshot, room_code, should_finalize) = {
        let mut guard = games.write().await;
        let session = guard
            .get_mut(&session_id)
            .ok_or_else(|| GameApiError::NotFound("对局不存在".to_string()))?;

        if session.status != GameStatus::Playing {
            return Err(GameApiError::Conflict("当前对局已结束".to_string()));
        }
        if session.current_player_id != participant.room_player_id {
            return Err(GameApiError::Forbidden("当前还没轮到你出牌".to_string()));
        }
        if payload.card_ids.is_empty() {
            return Err(GameApiError::BadRequest("请选择至少一张牌".to_string()));
        }

        let player = session
            .players
            .iter_mut()
            .find(|player| player.id == participant.room_player_id)
            .ok_or_else(|| GameApiError::NotFound("当前玩家不在对局中".to_string()))?;

        let mut played_cards = Vec::new();
        for card_id in &payload.card_ids {
            let index = player
                .hand_cards
                .iter()
                .position(|card| &card.id == card_id)
                .ok_or_else(|| GameApiError::BadRequest("所选手牌无效".to_string()))?;
            played_cards.push(player.hand_cards.remove(index));
        }

        let player_just_finished =
            player.hand_cards.is_empty() && player.finished_at_turn.is_none();
        if player_just_finished {
            player.finished_at_turn = Some(session.turn);
        }

        session.table.played_cards = played_cards.clone();
        session.table.pass_streak = 0;
        session.table.last_played_by = Some(participant.room_player_id.clone());
        session.last_action = Some(GameActionRecord {
            player_id: participant.room_player_id.clone(),
            action: "play_cards".to_string(),
            cards: played_cards,
            message: "玩家已出牌".to_string(),
            turn: session.turn,
        });

        if player_just_finished {
            session.status = GameStatus::Finished;
            session.winner_ids = vec![participant.room_player_id.clone()];
        } else {
            rotate_to_next_player(session);
        }

        (
            build_snapshot(session, &participant.room_player_id),
            session.room_code.clone(),
            matches!(session.status, GameStatus::Finished),
        )
    };

    if should_finalize {
        finalize_finished_game(&games, &rooms, &room_code).await;
    }

    Ok(Json(ApiResponse::success(snapshot)))
}

pub async fn skip_turn(
    State(user_repo): State<Arc<UserRepository>>,
    State(jwt_secret): State<JwtSecret>,
    State(games): State<SharedGameStore>,
    State(rooms): State<SharedRoomStore>,
    headers: HeaderMap,
    Path((session_id, _action_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<GameSnapshot>>, GameApiError> {
    let participant = resolve_current_participant(&headers, &jwt_secret, &user_repo).await?;
    let (snapshot, room_code, should_finalize) = {
        let mut guard = games.write().await;
        let session = guard
            .get_mut(&session_id)
            .ok_or_else(|| GameApiError::NotFound("对局不存在".to_string()))?;

        if session.status != GameStatus::Playing {
            return Err(GameApiError::Conflict("当前对局已结束".to_string()));
        }
        if session.current_player_id != participant.room_player_id {
            return Err(GameApiError::Forbidden("当前还没轮到你操作".to_string()));
        }
        if session.table.played_cards.is_empty() {
            return Err(GameApiError::Conflict("首个行动玩家不能跳过".to_string()));
        }

        session.table.pass_streak += 1;
        session.last_action = Some(GameActionRecord {
            player_id: participant.room_player_id.clone(),
            action: "skip".to_string(),
            cards: Vec::new(),
            message: "玩家选择跳过".to_string(),
            turn: session.turn,
        });

        let active_players = session
            .players
            .iter()
            .filter(|player| player.finished_at_turn.is_none())
            .count();
        if session.table.pass_streak >= active_players.saturating_sub(1) {
            session.table.played_cards.clear();
            session.table.pass_streak = 0;
            session.table.last_played_by = None;
        }

        rotate_to_next_player(session);

        (
            build_snapshot(session, &participant.room_player_id),
            session.room_code.clone(),
            matches!(session.status, GameStatus::Finished),
        )
    };

    if should_finalize {
        finalize_finished_game(&games, &rooms, &room_code).await;
    }

    Ok(Json(ApiResponse::success(snapshot)))
}

pub async fn end_game_for_room(games: &SharedGameStore, room_code: &str) {
    let mut guard = games.write().await;
    if let Some(session_id) = guard
        .values()
        .find(|session| session.room_code == room_code)
        .map(|session| session.session_id.clone())
    {
        guard.remove(&session_id);
    }
}
