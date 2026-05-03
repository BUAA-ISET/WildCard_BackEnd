use super::auth::TokenClaims;
use crate::domain::{
    room::{Room, RoomId, SharingCode},
    user::UserId,
};
use crate::error::AppError;
use crate::infrastructure::room::{CreateRoomOption, RoomRepository};
use axum::{
    Json,
    extract::{
        Query, State,
        ws::{self, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    #[serde(default)]
    pub room_password: String,
    pub player_capacity: usize,
}

#[derive(Debug, Serialize)]
pub struct RoomInfoResponse {
    pub room_id: RoomId,
    pub sharing_code: SharingCode,
    pub owner: UserId,
    pub players: Vec<Option<UserId>>,
}

impl From<&Room> for RoomInfoResponse {
    fn from(room: &Room) -> Self {
        Self {
            room_id: room.id(),
            sharing_code: room.sharing_code(),
            owner: room.owner,
            players: room.seats.0.clone(),
        }
    }
}

#[tracing::instrument]
pub async fn create_handler(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_repo): State<Arc<RoomRepository>>,
    Json(CreateRequest {
        room_password,
        player_capacity,
    }): Json<CreateRequest>,
) -> Result<Json<RoomInfoResponse>, AppError> {
    room_repo
        .create_room(
            user_id,
            CreateRoomOption {
                password: room_password,
                player_capacity,
            },
        )
        .map(|room| Json(RoomInfoResponse::from(room.value())))
}

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub room_id: RoomId,
}

#[tracing::instrument]
pub async fn delete_handler(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_repo): State<Arc<RoomRepository>>,
    Json(DeleteRequest { room_id }): Json<DeleteRequest>,
) -> Result<(), AppError> {
    room_repo.validate_owner(room_id, user_id)?;
    room_repo.delete_room(room_id)
}

#[derive(Debug, Deserialize)]
pub struct ReplaceOwnerRequest {
    pub room_id: RoomId,
    pub new_owner: UserId,
}

#[tracing::instrument]
pub async fn replace_owner_handler(
    TokenClaims { user_id, .. }: TokenClaims,
    State(room_repo): State<Arc<RoomRepository>>,
    Json(ReplaceOwnerRequest { room_id, new_owner }): Json<ReplaceOwnerRequest>,
) -> Result<(), AppError> {
    room_repo.validate_owner(room_id, user_id)?;
    room_repo.replace_owner(room_id, new_owner)
}

#[derive(Debug, Deserialize)]
pub struct FindRequest {
    pub room_id: Option<RoomId>,
    pub sharing_code: Option<SharingCode>,
    #[serde(default)]
    pub password: String,
}

#[tracing::instrument]
pub async fn find_handler(
    Query(FindRequest {
        room_id,
        sharing_code,
        password,
    }): Query<FindRequest>,
    State(room_repo): State<Arc<RoomRepository>>,
) -> Result<Json<RoomInfoResponse>, AppError> {
    // Helper function (lambda)
    let process_room = |room: &Room| {
        if room.password != password {
            return Err(AppError::InvalidPassword);
        }
        let response = RoomInfoResponse::from(room);
        let json = Json(response);
        Ok(json)
    };

    match (room_id, sharing_code) {
        (None, Some(sharing_code)) => room_repo
            .rooms
            .get_by_sharing_code(sharing_code)
            .and_then(|room_entry| process_room(room_entry.value())),
        (Some(room_id), None) => room_repo
            .rooms
            .get_by_id(room_id)
            .and_then(|room_entry| process_room(room_entry.value())),
        _ => {
            return Err(AppError::InvalidInput(
                "room_id 与 sharing_code 只能其中一个有值".to_string(),
            ));
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EnterRequest {
    pub room_id: RoomId,
    pub seat_index: usize,
    #[serde(default)]
    pub password: String,
}

// Web socket
#[tracing::instrument]
pub async fn enter_handler(
    TokenClaims { user_id, .. }: TokenClaims,
    Query(EnterRequest {
        room_id,
        seat_index,
        password,
    }): Query<EnterRequest>,
    State(room_repo): State<Arc<RoomRepository>>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    room_repo.validate_password(room_id, password)?;
    room_repo.take_seat(room_id, user_id, seat_index)?;

    Ok(ws.on_upgrade(move |mut socket| async move {
        // Subscribe to the room's message
        let mut rx = {
            let rx_option = room_repo.subscribe_broadcast(room_id);
            match rx_option {
                Ok(rx) => rx,
                Err(_) => {
                    tracing::warn!("Room #{room_id} disappeared!");
                    return;
                }
            }
        };

        let (mut sender, mut receiver) = socket.split();

        // Send the room's broadcast to client
        let mut send_task = tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if sender.send(ws::Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        });

        // Receive actions from client
        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(axum::extract::ws::Message::Text(text))) = receiver.next().await {
                // 这里运行你的【规则引擎解释器】逻辑
                // 如果逻辑导致状态变更，直接调用 tx.send(text)
                // 注意：此处需要获取 room_repo 的锁来更新房间内玩家的数据状态
            }
        });

        // Wait for the connection to end. Handle cleanup logic.
        tokio::select! {
            _ = (&mut send_task) => recv_task.abort(),
            _ = (&mut recv_task) => send_task.abort(),
        }
    }))
}
