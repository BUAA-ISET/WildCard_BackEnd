use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
};
use sqlx::{PgPool, Row};
use tokio::sync::RwLock;

use crate::{
    domain::{
        replay::{
            MatchHistoryRecord, MatchReplay, MatchResult, ReplayAction, ReplayCard,
            ReplayCardDisplay, ReplayFrame, ReplayPlayer,
        },
        room::Room,
        rule_engine::{GameCard, GameSession},
    },
    error::AppError,
    interface::{auth::TokenClaims, user::ApiResponse},
};

pub type ReplayStore = Arc<RwLock<ReplayRepository>>;

#[derive(Debug, Default)]
pub struct ReplayRepository {
    pub replays: HashMap<String, MatchReplay>,
    pub session_replay_ids: HashMap<String, String>,
}

pub fn build_replay_store() -> ReplayStore {
    Arc::new(RwLock::new(ReplayRepository::default()))
}

#[derive(Clone)]
pub struct ReplayPersistence {
    pub pool: PgPool,
}

impl ReplayPersistence {
    pub async fn ensure_schema(&self) -> Result<(), AppError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS match_replays (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                room_code TEXT NOT NULL,
                player_ids TEXT[] NOT NULL,
                replay JSONB NOT NULL,
                started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|error| tracing::warn!("Database error {error}"))?;

        Ok(())
    }

    pub async fn save(&self, replay: &MatchReplay) -> Result<(), AppError> {
        let replay_value = serde_json::to_value(replay).map_err(AppError::JsonError)?;
        let player_ids = replay
            .record
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect::<Vec<_>>();

        sqlx::query(
            r#"
            INSERT INTO match_replays (
                id, session_id, room_code, player_ids, replay, started_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            ON CONFLICT (id) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                room_code = EXCLUDED.room_code,
                player_ids = EXCLUDED.player_ids,
                replay = EXCLUDED.replay,
                updated_at = NOW()
            "#,
        )
        .bind(&replay.record.id)
        .bind(&replay.record.session_id)
        .bind(&replay.record.room_code)
        .bind(player_ids)
        .bind(replay_value)
        .execute(&self.pool)
        .await
        .inspect_err(|error| tracing::warn!("Database error {error}"))?;

        Ok(())
    }

    pub async fn list_for_player(&self, player_id: &str) -> Result<Vec<MatchReplay>, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT replay
            FROM match_replays
            WHERE $1 = ANY(player_ids)
            ORDER BY updated_at DESC
            "#,
        )
        .bind(player_id)
        .fetch_all(&self.pool)
        .await
        .inspect_err(|error| tracing::warn!("Database error {error}"))?;

        rows.into_iter()
            .map(|row| decode_replay(row.get("replay")))
            .collect()
    }

    pub async fn get(&self, replay_id: &str) -> Result<Option<MatchReplay>, AppError> {
        let row = sqlx::query(
            r#"
            SELECT replay
            FROM match_replays
            WHERE id = $1
            "#,
        )
        .bind(replay_id)
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|error| tracing::warn!("Database error {error}"))?;

        row.map(|row| decode_replay(row.get("replay"))).transpose()
    }
}

pub async fn list_history(
    TokenClaims { user_id, .. }: TokenClaims,
    State(replay_persistence): State<ReplayPersistence>,
) -> Result<Json<ApiResponse<Vec<MatchHistoryRecord>>>, AppError> {
    let player_id = user_id.to_string();
    let mut records = replay_persistence
        .list_for_player(&player_id)
        .await?
        .into_iter()
        .filter(|replay| replay.record.includes_player(&player_id))
        .map(|replay| replay.record.clone().with_result_for_player(&player_id))
        .collect::<Vec<_>>();

    records.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(Json(ApiResponse::success(records)))
}

pub async fn get_replay(
    TokenClaims { user_id, .. }: TokenClaims,
    State(replay_persistence): State<ReplayPersistence>,
    Path(replay_id): Path<String>,
) -> Result<Json<ApiResponse<MatchReplay>>, AppError> {
    let player_id = user_id.to_string();
    let replay = replay_persistence
        .get(&replay_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !replay.record.includes_player(&player_id) {
        return Err(AppError::Unauthorized(
            "不能查看未参与对局的回放".to_string(),
        ));
    }

    let mut replay = replay.clone();
    replay.record = replay.record.with_result_for_player(&player_id);
    Ok(Json(ApiResponse::success(replay)))
}

pub async fn start_match_replay_with_persistence(
    replay_store: &ReplayStore,
    replay_persistence: Option<&ReplayPersistence>,
    session: &GameSession,
    room: &Room,
) {
    let replay_id = replay_id_for_session(&session.id);
    let started_at = now_iso_string();
    let record = MatchHistoryRecord {
        id: replay_id.clone(),
        session_id: session.id.clone(),
        room_code: session.room_code.clone(),
        rule_id: room.rule_id.clone(),
        rule_name: room.rule_name.clone(),
        started_at: started_at.clone(),
        ended_at: started_at,
        result: MatchResult::Draw,
        players: room.players.iter().map(to_replay_player).collect(),
        winner_ids: Vec::new(),
    };
    let mut replay = MatchReplay {
        record,
        frames: Vec::new(),
    };
    replay.frames.push(build_frame(session, 0, None));

    let mut guard = replay_store.write().await;
    guard
        .session_replay_ids
        .insert(session.id.clone(), replay_id.clone());
    guard.replays.insert(replay_id, replay.clone());
    drop(guard);

    if let Some(replay_persistence) = replay_persistence
        && let Err(error) = replay_persistence.save(&replay).await
    {
        tracing::warn!("Failed to persist match replay: {error}");
    }
}

pub async fn append_match_replay_frame_with_persistence(
    replay_store: &ReplayStore,
    replay_persistence: Option<&ReplayPersistence>,
    session: &GameSession,
    room: Option<&Room>,
) {
    let mut guard = replay_store.write().await;
    let replay_id = guard
        .session_replay_ids
        .get(&session.id)
        .cloned()
        .unwrap_or_else(|| replay_id_for_session(&session.id));

    let replay = guard.replays.entry(replay_id.clone()).or_insert_with(|| {
        let started_at = now_iso_string();
        MatchReplay {
            record: MatchHistoryRecord {
                id: replay_id.clone(),
                session_id: session.id.clone(),
                room_code: session.room_code.clone(),
                rule_id: room.map(|room| room.rule_id.clone()).unwrap_or_default(),
                rule_name: session.rule_name.clone(),
                started_at: started_at.clone(),
                ended_at: started_at,
                result: MatchResult::Draw,
                players: room
                    .map(|room| room.players.iter().map(to_replay_player).collect())
                    .unwrap_or_else(|| {
                        session
                            .players
                            .iter()
                            .map(|player| ReplayPlayer {
                                id: player.id.clone(),
                                username: format!("玩家{}", player.runtime_index + 1),
                                avatar: String::new(),
                            })
                            .collect()
                    }),
                winner_ids: Vec::new(),
            },
            frames: Vec::new(),
        }
    });

    let index = replay.frames.len() as u32;
    let action = build_action(session);
    replay.frames.push(build_frame(session, index, action));
    replay.record.ended_at = now_iso_string();
    if session.status == "finished" {
        replay.record.winner_ids = winner_ids(session);
    }
    let replay_to_save = replay.clone();
    drop(guard);

    if let Some(replay_persistence) = replay_persistence
        && let Err(error) = replay_persistence.save(&replay_to_save).await
    {
        tracing::warn!("Failed to persist match replay: {error}");
    }
}

fn decode_replay(value: serde_json::Value) -> Result<MatchReplay, AppError> {
    serde_json::from_value(value).map_err(AppError::JsonError)
}

fn build_frame(session: &GameSession, index: u32, action: Option<ReplayAction>) -> ReplayFrame {
    let current_player_id = session
        .pending_action
        .as_ref()
        .map(|pending| pending.player_id.clone())
        .unwrap_or_else(|| {
            let player_index = session.table.get("player_index").copied().unwrap_or_default();
            session
                .players
                .get(player_index.max(0) as usize)
                .map(|player| player.id.clone())
                .unwrap_or_default()
        });
    let table_cards = if session.last_action_skipped {
        Vec::new()
    } else {
        session
            .last_successful_play
            .as_ref()
            .map(|play| play.cards.iter().map(to_replay_card).collect())
            .unwrap_or_default()
    };
    let hands = session
        .hands
        .iter()
        .map(|(player_id, cards)| {
            (
                player_id.clone(),
                cards.iter().map(to_replay_card).collect::<Vec<_>>(),
            )
        })
        .collect();

    ReplayFrame {
        index,
        elapsed_seconds: index.saturating_mul(15),
        current_player_id,
        hands,
        table_cards,
        action,
    }
}

fn build_action(session: &GameSession) -> Option<ReplayAction> {
    session.last_action_player_id.as_ref().map(|player_id| {
        let skipped = session.last_action_skipped;
        let cards = session
            .last_action_cards
            .iter()
            .map(to_replay_card)
            .collect::<Vec<_>>();
        ReplayAction {
            player_id: player_id.clone(),
            action: if skipped { "skip" } else { "playCards" }.to_string(),
            message: if skipped {
                "选择跳过".to_string()
            } else {
                format!("打出了 {} 张牌", cards.len())
            },
            cards,
            turn: session.execution_log.len() as u32,
        }
    })
}

fn to_replay_player(player: &crate::domain::room::Player) -> ReplayPlayer {
    ReplayPlayer {
        id: player.id.clone(),
        username: player.username.clone(),
        avatar: player.avatar.clone(),
    }
}

fn to_replay_card(card: &GameCard) -> ReplayCard {
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

    ReplayCard {
        id: card.id.clone(),
        properties: card.properties.clone(),
        display: ReplayCardDisplay {
            rank: rank_display(point),
            suit: suit_display(suit),
        },
    }
}

fn winner_ids(session: &GameSession) -> Vec<String> {
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

fn replay_id_for_session(session_id: &str) -> String {
    format!("replay-{session_id}")
}

fn now_iso_string() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
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
