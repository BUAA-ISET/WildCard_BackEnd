use super::auth::TokenClaims;
use crate::domain::{
    room::{Room, RoomId, SharingCode},
    rule::{RuleDefinition, RuleRuntimeEvent, RuleValue},
    user::UserId,
};
use crate::error::AppError;
use crate::infrastructure::room::{CreateRoomOption, RoomRepository};
use axum::{
    Json,
    extract::{
        Query, State,
        ws::{self, WebSocketUpgrade},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    #[serde(default)]
    pub room_password: String,
    pub player_capacity: usize,
    #[serde(default)]
    pub rule: Option<RuleDefinition>,
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
        rule,
    }): Json<CreateRequest>,
) -> Result<Json<RoomInfoResponse>, AppError> {
    room_repo
        .create_room(
            user_id,
            CreateRoomOption {
                password: room_password,
                player_capacity,
                rule,
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

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientRoomMessage {
    Heartbeat,
    Leave,
    Emit {
        name: String,
        #[serde(default)]
        payload: BTreeMap<String, RuleValue>,
    },
    Command {
        name: String,
        #[serde(default)]
        payload: BTreeMap<String, RuleValue>,
    },
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

    Ok(ws.on_upgrade(move |socket| async move {
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

        if let Ok(snapshot) = room_repo.snapshot(room_id) {
            let payload = crate::domain::room::RoomEvent::Snapshot(snapshot);
            if let Ok(serialized) = serde_json::to_string(&payload) {
                let _ = sender.send(ws::Message::Text(serialized.into())).await;
            }
        }

        loop {
            tokio::select! {
                received = rx.recv() => {
                    match received {
                        Ok(msg) => {
                            if sender.send(ws::Message::Text(msg.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                incoming = receiver.next() => {
                    match incoming {
                        Some(Ok(axum::extract::ws::Message::Text(text))) => {
                            let message = match serde_json::from_str::<ClientRoomMessage>(&text) {
                                Ok(message) => message,
                                Err(error) => {
                                    let _ = room_repo.publish_event(
                                        room_id,
                                        crate::domain::room::RoomEvent::Error {
                                            room_id,
                                            message: error.to_string(),
                                        },
                                    );
                                    continue;
                                }
                            };

                            match message {
                                ClientRoomMessage::Heartbeat => {
                                    if let Ok(snapshot) = room_repo.snapshot(room_id) {
                                        let _ = room_repo.publish_event(
                                            room_id,
                                            crate::domain::room::RoomEvent::StateChanged {
                                                room_id,
                                                snapshot,
                                            },
                                        );
                                    }
                                }
                                ClientRoomMessage::Leave => break,
                                ClientRoomMessage::Emit { name, payload }
                                | ClientRoomMessage::Command { name, payload } => {
                                    let event = RuleRuntimeEvent { name, payload };
                                    let _ = room_repo.push_runtime_event(room_id, event);
                                }
                            }
                        }
                        Some(Ok(axum::extract::ws::Message::Close(_))) | None => break,
                        Some(Ok(_)) => continue,
                        Some(Err(_)) => break,
                    }
                }
            }
        }

        let _ = room_repo.leave_seat(room_id, user_id);
    }))
}
