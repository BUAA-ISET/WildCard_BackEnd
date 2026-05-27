// pub mod domain;
// pub mod error;
// pub mod infrastructure;
// pub mod interface;
// pub mod state;

// use serde::{Deserialize, Serialize};
// use std::collections::BTreeSet;

// #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// pub struct TestUser {
//     pub id: u64,
//     pub username: String,
//     pub email: String,
// }

// #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// pub struct TestRuleDefinition {
//     pub id: String,
//     pub name: String,
//     pub version: u32,
//     pub steps: Vec<String>,
// }

// pub fn healthcheck() -> &'static str {
//     "ok"
// }

// pub mod api {
//     use std::collections::{HashMap, HashSet};

//     #[derive(Debug, Clone, PartialEq, Eq)]
//     pub struct ApiResponse {
//         pub status_code: u16,
//         pub message: String,
//     }

//     impl ApiResponse {
//         pub fn is_ok(&self) -> bool {
//             (200..300).contains(&self.status_code)
//         }
//     }

//     #[derive(Debug, Default)]
//     pub struct TestApp {
//         users: HashMap<String, String>,
//         rooms: HashSet<String>,
//         current_user: Option<String>,
//     }

//     impl TestApp {
//         pub fn new() -> Self {
//             Self::default()
//         }

//         pub fn register_user(&mut self, username: &str) -> ApiResponse {
//             if username.trim().is_empty() {
//                 return ApiResponse {
//                     status_code: 400,
//                     message: String::from("username is required"),
//                 };
//             }

//             if self.users.contains_key(username) {
//                 return ApiResponse {
//                     status_code: 409,
//                     message: String::from("user already exists"),
//                 };
//             }

//             self.users
//                 .insert(username.to_string(), "password".to_string());
//             ApiResponse {
//                 status_code: 201,
//                 message: format!("user {username} created"),
//             }
//         }

//         pub fn register_user_with_password(
//             &mut self,
//             username: &str,
//             password: &str,
//         ) -> ApiResponse {
//             if username.trim().is_empty() {
//                 return ApiResponse {
//                     status_code: 400,
//                     message: String::from("username is required"),
//                 };
//             }

//             if password.trim().is_empty() {
//                 return ApiResponse {
//                     status_code: 400,
//                     message: String::from("password is required"),
//                 };
//             }

//             if self.users.contains_key(username) {
//                 return ApiResponse {
//                     status_code: 409,
//                     message: String::from("user already exists"),
//                 };
//             }

//             self.users
//                 .insert(username.to_string(), password.to_string());
//             ApiResponse {
//                 status_code: 201,
//                 message: format!("user {username} created"),
//             }
//         }

//         pub fn login_user(&mut self, username: &str, password: &str) -> ApiResponse {
//             if username.trim().is_empty() {
//                 return ApiResponse {
//                     status_code: 400,
//                     message: String::from("username is required"),
//                 };
//             }

//             if password.trim().is_empty() {
//                 return ApiResponse {
//                     status_code: 400,
//                     message: String::from("password is required"),
//                 };
//             }

//             match self.users.get(username) {
//                 Some(stored_password) if stored_password == password => {
//                     self.current_user = Some(username.to_string());
//                     ApiResponse {
//                         status_code: 200,
//                         message: format!("user {username} logged in"),
//                     }
//                 }
//                 Some(_) => ApiResponse {
//                     status_code: 401,
//                     message: String::from("invalid password"),
//                 },
//                 None => ApiResponse {
//                     status_code: 401,
//                     message: String::from("user not found"),
//                 },
//             }
//         }

//         pub fn logout(&mut self) -> ApiResponse {
//             self.current_user = None;
//             ApiResponse {
//                 status_code: 200,
//                 message: String::from("logged out"),
//             }
//         }

//         pub fn get_current_user(&self) -> Option<&str> {
//             self.current_user.as_deref()
//         }

//         pub fn find_user(&self, username: &str) -> ApiResponse {
//             if self.users.contains_key(username) {
//                 ApiResponse {
//                     status_code: 200,
//                     message: format!("user {username} found"),
//                 }
//             } else {
//                 ApiResponse {
//                     status_code: 404,
//                     message: String::from("user not found"),
//                 }
//             }
//         }

//         pub fn create_room(&mut self, room_id: &str) -> ApiResponse {
//             if room_id.trim().is_empty() {
//                 return ApiResponse {
//                     status_code: 400,
//                     message: String::from("room id is required"),
//                 };
//             }

//             if !self.rooms.insert(room_id.to_string()) {
//                 return ApiResponse {
//                     status_code: 409,
//                     message: String::from("room already exists"),
//                 };
//             }

//             ApiResponse {
//                 status_code: 201,
//                 message: format!("room {room_id} created"),
//             }
//         }

//         pub fn validate_rule_definition(&self, body: &str) -> ApiResponse {
//             if body.trim().is_empty() {
//                 return ApiResponse {
//                     status_code: 400,
//                     message: String::from("rule payload is required"),
//                 };
//             }

//             if body.contains("\"name\"") {
//                 return ApiResponse {
//                     status_code: 200,
//                     message: String::from("rule payload accepted"),
//                 };
//             }

//             ApiResponse {
//                 status_code: 422,
//                 message: String::from("rule payload is invalid"),
//             }
//         }
//     }
// }

// pub mod websocket {
//     use super::BTreeSet;

//     #[derive(Debug, Clone, PartialEq, Eq)]
//     pub struct RoomEvent {
//         pub room_id: String,
//         pub user_id: String,
//         pub action: &'static str,
//     }

//     #[derive(Debug, Default)]
//     pub struct RoomSession {
//         room_id: String,
//         participants: BTreeSet<String>,
//     }

//     impl RoomSession {
//         pub fn new(room_id: &str) -> Self {
//             Self {
//                 room_id: room_id.to_string(),
//                 participants: BTreeSet::new(),
//             }
//         }

//         pub fn join(&mut self, user_id: &str) -> RoomEvent {
//             self.participants.insert(user_id.to_string());
//             RoomEvent {
//                 room_id: self.room_id.clone(),
//                 user_id: user_id.to_string(),
//                 action: "joined",
//             }
//         }

//         pub fn leave(&mut self, user_id: &str) -> RoomEvent {
//             self.participants.remove(user_id);
//             RoomEvent {
//                 room_id: self.room_id.clone(),
//                 user_id: user_id.to_string(),
//                 action: "left",
//             }
//         }

//         pub fn participant_count(&self) -> usize {
//             self.participants.len()
//         }

//         pub fn participants(&self) -> Vec<String> {
//             self.participants.iter().cloned().collect()
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn healthcheck_returns_ok() {
//         assert_eq!(healthcheck(), "ok");
//     }
// }
