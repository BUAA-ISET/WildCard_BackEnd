use crate::domain::{
    room::{Room, RoomId, RoomTable, SharingCode},
    user::UserId,
};
use crate::error::AppError;
use dashmap::DashMap;
use rand::RngExt;
use tokio::sync::broadcast;

#[derive(Default, Debug)]
pub struct RoomRepository {
    pub rooms: RoomTable,
    sharing_codes: DashMap<SharingCode, RoomId>,
}

impl RoomRepository {
    fn alloc_sharing_code(&self) -> Option<SharingCode> {
        if self.rooms.len() >= 1_000_000 {
            tracing::error!("Sharing code full!");
            return None;
        }

        let mut rng = rand::rng();
        loop {
            let val = rng.random_range(0..1_000_000);
            let code = SharingCode(val);
            if self.sharing_codes.get(&code).is_none() {
                return Some(code);
            }
        }
    }

    pub fn create_room(
        &'_ self,
        owner: UserId,
        CreateRoomOption {
            password,
            player_capacity,
        }: CreateRoomOption,
    ) -> Result<dashmap::mapref::one::Ref<'_, RoomId, Room>, AppError> {
        let sharing_code = self
            .alloc_sharing_code()
            .ok_or(AppError::SharingCodeRunOut)?;
        let room = Room::new(sharing_code, password, owner, player_capacity);
        Ok(self.rooms.insert(room))
    }

    pub fn delete_room(&self, room_id: RoomId) -> Result<(), AppError> {
        self.rooms.remove_by_id(room_id)?;
        Ok(())
    }

    pub fn replace_owner(&self, room_id: RoomId, new_owner: UserId) -> Result<(), AppError> {
        self.rooms.get_mut_by_id(room_id)?.owner = new_owner;
        Ok(())
    }

    pub fn validate_owner(&self, room_id: RoomId, user_id: UserId) -> Result<(), AppError> {
        if self.rooms.get_by_id(room_id)?.owner != user_id {
            return Err(AppError::Unauthorized("Not the room owner".to_string()));
        }
        Ok(())
    }

    pub fn validate_password(&self, room_id: RoomId, password: String) -> Result<(), AppError> {
        if self.rooms.get_by_id(room_id)?.password != password {
            Err(AppError::InvalidPassword)
        } else {
            Ok(())
        }
    }

    pub fn take_seat(
        &self,
        room_id: RoomId,
        user_id: UserId,
        seat_index: usize,
    ) -> Result<(), AppError> {
        self.rooms
            .get_mut_by_id(room_id)?
            .seats
            .assign(seat_index, user_id)
    }

    pub fn leave_seat(&self, room_id: RoomId, user_id: UserId) -> Result<(), AppError> {
        self.rooms.get_mut_by_id(room_id)?.seats.remove(user_id)
    }

    pub fn subscribe_broadcast(
        &self,
        room_id: RoomId,
    ) -> Result<broadcast::Receiver<String>, AppError> {
        Ok(self.rooms.get_by_id(room_id)?.tx.subscribe())
    }
}

pub struct CreateRoomOption {
    pub password: String,
    pub player_capacity: usize,
}
