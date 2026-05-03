use super::user::UserId;
use crate::error::AppError;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Debug, Display},
    hash::Hash,
};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(PartialEq, PartialOrd, Eq, Clone, Copy, Serialize, Deserialize, Debug, Hash)]
pub struct RoomId(pub Uuid);

impl Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RoomId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone)]
pub struct Room {
    /// Id for room used as primary key, which should be unique in the whole time.
    id: RoomId,
    /// A 6-digits code, which should be unique in a short period.
    /// For example: `123-456`
    sharing_code: SharingCode,
    /// Password for entering the room.
    /// Set to `""` if the password is not needed.
    pub password: String,
    /// The owner of the room, usually the same as the creator of the room.
    pub owner: UserId,
    /// Players list in order. `None` means the seat is currently empty.
    pub seats: Seats,
    /// Broadcast sender
    pub tx: broadcast::Sender<String>,
}

impl Room {
    pub fn new(
        sharing_code: SharingCode,
        password: String,
        owner: UserId,
        player_capacity: usize,
    ) -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            id: RoomId::new(),
            sharing_code,
            password,
            owner,
            seats: Seats::new(player_capacity),
            tx,
        }
    }

    pub fn is_owner(&self, user_id: UserId) -> bool {
        self.owner == user_id
    }

    /// Expose read only field `id`
    pub fn id(&self) -> RoomId {
        self.id
    }

    /// Expose read only field `sharing_code`
    pub fn sharing_code(&self) -> SharingCode {
        self.sharing_code
    }
}

#[derive(Debug, Clone)]
pub struct Seats(pub Vec<Option<UserId>>);

impl Seats {
    pub fn new(capacity: usize) -> Self {
        Self(vec![None; capacity])
    }

    pub fn capacity(&self) -> usize {
        self.0.len()
    }

    pub fn contains(&self, player: UserId) -> bool {
        self.0.contains(&Some(player))
    }

    pub fn find(&self, player: UserId) -> Option<usize> {
        self.0
            .iter()
            // Add index for each item
            .enumerate()
            // Filter out not none value: [None, Some(p1), Some(p2)] ==> [p1, p2]
            .filter_map(|(i, pid_option)| pid_option.as_ref().map(|pid| (i, pid)))
            // Find the given player
            .find(|&(_i, pid)| *pid == player)
            // Extract index
            .map(|(i, _pid)| i)
    }

    pub fn find_mut(&mut self, player: UserId) -> Option<(usize, &mut Option<UserId>)> {
        self.0
            .iter_mut()
            // Add index for each item
            .enumerate()
            // Filter out not none value: [None, Some(p1), Some(p2)] ==> [p1, p2]
            .filter_map(|(i, pid_option)| pid_option.map(|_| (i, pid_option)))
            // Find the given player
            .find(|&(_i, &mut pid)| pid == Some(player))
    }

    pub fn count(&self) -> usize {
        self.0
            .iter()
            .map(|play| match play {
                Some(_) => 1,
                None => 0,
            })
            .sum()
    }

    pub fn is_full(&self) -> bool {
        self.0.iter().all(Option::is_some)
    }

    pub fn get(&self, seat_index: usize) -> Result<&Option<UserId>, AppError> {
        self.0.get(seat_index).ok_or(AppError::InvalidInput(format!(
            "Seat #{seat_index} out of bound"
        )))
    }

    pub fn get_mut(&mut self, seat_index: usize) -> Result<&mut Option<UserId>, AppError> {
        self.0
            .get_mut(seat_index)
            .ok_or(AppError::InvalidInput(format!(
                "Seat #{seat_index} out of bound"
            )))
    }

    pub fn assign(&mut self, index: usize, player: UserId) -> Result<(), AppError> {
        if self.contains(player) {
            return Err(AppError::InvalidInput(format!("Player already in seats")));
        }

        let seat = self.get_mut(index)?;

        if seat.is_some() {
            return Err(AppError::InvalidInput(format!(
                "Seat #{index} is occupied by others"
            )));
        }
        *seat = Some(player);
        Ok(())
    }

    pub fn remove(&mut self, player: UserId) -> Result<(), AppError> {
        let find = self.find_mut(player);
        match find {
            Some((_i, p)) => *p = None,
            None => return Err(AppError::InvalidInput(format!("Player not in room"))),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct SharingCode(pub u32);

impl Display for SharingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

pub struct RoomTable {
    rooms: DashMap<RoomId, Room>,
    sharing_codes: DashMap<SharingCode, RoomId>,
}

impl Default for RoomTable {
    fn default() -> Self {
        const CAPACITY: usize = 1024;

        Self {
            rooms: DashMap::with_capacity(CAPACITY),
            sharing_codes: DashMap::with_capacity(CAPACITY),
        }
    }
}

impl RoomTable {
    pub fn insert(&'_ self, room: Room) -> dashmap::mapref::one::Ref<'_, RoomId, Room> {
        let room_id = room.id;
        self.sharing_codes.insert(room.sharing_code, room_id);
        self.rooms.entry(room_id).or_insert(room).downgrade()
    }

    pub fn remove_by_id(&self, room_id: RoomId) -> Result<Room, AppError> {
        let (_room_id, room) = self.rooms.remove(&room_id).ok_or(AppError::NotFound)?;
        self.sharing_codes.remove(&room.sharing_code);
        Ok(room)
    }

    pub fn get_by_id(
        &'_ self,
        room_id: RoomId,
    ) -> Result<dashmap::mapref::one::Ref<'_, RoomId, Room>, AppError> {
        self.rooms.get(&room_id).ok_or(AppError::NotFound)
    }

    pub fn get_by_sharing_code(
        &'_ self,
        sharing_code: SharingCode,
    ) -> Result<dashmap::mapref::one::Ref<'_, RoomId, Room>, AppError> {
        let room_id = self
            .sharing_codes
            .get(&sharing_code)
            .map(|r| *r.value())
            .ok_or(AppError::NotFound)?;
        self.rooms.get(&room_id).ok_or(AppError::NotFound)
    }

    pub fn get_mut_by_id(
        &'_ self,
        room_id: RoomId,
    ) -> Result<dashmap::mapref::one::RefMut<'_, RoomId, Room>, AppError> {
        self.rooms.get_mut(&room_id).ok_or(AppError::NotFound)
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }
}

impl Debug for RoomTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoomTable")
            .field("len", &self.len())
            .finish()
    }
}
