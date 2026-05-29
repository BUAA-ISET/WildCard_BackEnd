#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

const SUPPORTED_COMPONENT_TYPES: &[u16] = &[
    1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
    28, 29, 30,
];
const FLOW_STEP_LIMIT: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleProperty {
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default)]
    pub default: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMethod {
    #[serde(default)]
    pub parameters: HashMap<String, RuleMethodParameter>,
    pub returns: Option<String>,
    #[serde(default)]
    pub flow: FlowGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMethodParameter {
    #[serde(rename = "type")]
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleClass {
    #[serde(default)]
    pub default_properties: HashMap<String, RuleProperty>,
    #[serde(default)]
    pub user_properties: HashMap<String, RuleProperty>,
    #[serde(default)]
    pub methods: HashMap<String, RuleMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCardset {
    pub name: String,
    #[serde(default)]
    pub properties: HashMap<String, RuleProperty>,
    #[serde(default)]
    pub build_flow: FlowGraph,
    #[serde(default)]
    pub compare_flow: FlowGraph,
    #[serde(default)]
    pub successors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCardsetComparison {
    #[serde(rename = "cardsetA")]
    pub cardset_a: String,
    #[serde(rename = "cardsetB")]
    pub cardset_b: String,
    #[serde(default)]
    pub compare_flow: FlowGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedRuleDesign {
    #[serde(default)]
    pub classes: HashMap<String, RuleClass>,
    #[serde(default)]
    pub cardsets: HashMap<String, RuleCardset>,
    #[serde(default)]
    pub cardset_comparisons: HashMap<String, RuleCardsetComparison>,
    #[serde(default)]
    pub match_flow: FlowGraph,
    #[serde(default)]
    pub end_flow: FlowGraph,
}

pub type FlowGraph = HashMap<String, FlowNode>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowNode {
    #[serde(rename = "type")]
    pub component_type: u16,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<FlowNodePosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowNodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeRule {
    pub name: String,
    pub player_count: u8,
    pub description: String,
    pub design: ExportedRuleDesign,
    pub match_flow: RuntimeFlow,
    pub end_flow: RuntimeFlow,
    pub cardset_flows: HashMap<String, RuntimeCardset>,
    pub comparison_flows: HashMap<String, RuntimeComparison>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeCardset {
    pub id: String,
    pub name: String,
    pub build_flow: RuntimeFlow,
    pub compare_flow: RuntimeFlow,
    pub successors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeComparison {
    pub cardset_a: String,
    pub cardset_b: String,
    pub compare_flow: RuntimeFlow,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeFlow {
    pub entry: String,
    pub nodes: HashMap<String, RuntimeNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeNode {
    pub id: String,
    pub component_type: u16,
    pub content: Option<Value>,
    pub count: Option<Value>,
    pub transitions: Vec<RuntimeTransition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeTransition {
    pub label: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameSession {
    pub id: String,
    pub room_code: String,
    pub rule_name: String,
    pub player_count: u8,
    pub status: String,
    pub active_flow: String,
    pub current_node: String,
    pub players: Vec<GamePlayer>,
    pub table: HashMap<String, i64>,
    pub deck: Vec<GameCard>,
    pub hands: HashMap<String, Vec<GameCard>>,
    pub discard_pile: Vec<GameCard>,
    pub pending_action: Option<PendingAction>,
    pub settlement_results: HashMap<String, i64>,
    pub execution_log: Vec<String>,
    pub last_successful_play: Option<LastSuccessfulPlay>,
    pub last_action_player_id: Option<String>,
    pub last_action_cards: Vec<GameCard>,
    pub last_action_skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LastSuccessfulPlay {
    pub player_id: String,
    pub cards: Vec<GameCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GamePlayer {
    pub id: String,
    pub properties: HashMap<String, i64>,
    #[serde(skip_serializing)]
    pub runtime_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameCard {
    pub id: String,
    pub properties: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingAction {
    pub id: String,
    pub player_id: String,
    pub component_type: u16,
    pub timer: u64,
    pub options: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerActionInput {
    #[serde(default, alias = "cardIds")]
    pub cards: Vec<String>,
    #[serde(default)]
    pub choice: Option<i64>,
}

#[derive(Debug, Clone)]
enum EvalValue {
    Int(i64),
    Ints(Vec<i64>),
    Bool(bool),
    Table,
    Player(GamePlayer),
    PlayerIndex(usize),
    Card(GameCard),
    Cards(Vec<GameCard>),
    Players(Vec<GamePlayer>),
    Cardset(CardsetRuntimeResult),
    Choice(i64),
    None,
}

#[derive(Debug, Clone)]
struct PlayResolution {
    cardset_id: String,
    cardset_name: String,
    cards: Vec<GameCard>,
    properties: HashMap<String, i64>,
}

#[derive(Debug, Clone)]
struct CardsetRuntimeResult {
    cardset_id: String,
    cardset_name: String,
    cards: Vec<GameCard>,
    properties: HashMap<String, i64>,
}

#[derive(Debug, Clone)]
struct CardsetBuildResult {
    matched: bool,
    properties: HashMap<String, i64>,
}

#[derive(Debug, Clone)]
enum CompareFlowResult {
    A,
    B,
}

#[derive(Debug, Clone)]
struct CompareContext {
    cardset_a: CardsetRuntimeResult,
    cardset_b: CardsetRuntimeResult,
}

#[derive(Debug, Clone)]
struct MethodContext {
    object_ref: String,
    parameters: HashMap<String, EvalValue>,
}

#[derive(Debug, Clone, Copy)]
enum FlowKind {
    Match,
    End,
    CardsetBuild,
    CardsetCompare,
    Method,
}

#[derive(Debug, Clone)]
struct RuntimeEvalContext<'a> {
    runtime_rule: &'a RuntimeRule,
    flow: &'a RuntimeFlow,
    flow_kind: FlowKind,
    compare: Option<&'a CompareContext>,
    method: Option<&'a MethodContext>,
    cardset_input: Option<&'a [GameCard]>,
    cardset_id: Option<&'a str>,
    current_player_override: Option<String>,
}

pub struct RuleEngine;

impl RuleEngine {
    pub fn parse(
        name: String,
        player_count: u8,
        description: String,
        design: ExportedRuleDesign,
    ) -> Result<RuntimeRule, AppError> {
        validate_metadata(&name, player_count)?;
        validate_design(&design)?;

        let match_flow = compile_flow("match_flow", &design.match_flow)?;
        let end_flow = compile_flow("end_flow", &design.end_flow)?;

        let cardset_flows = design
            .cardsets
            .iter()
            .map(|(id, cardset)| {
                Ok((
                    id.clone(),
                    RuntimeCardset {
                        id: id.clone(),
                        name: cardset.name.clone(),
                        build_flow: compile_flow(
                            &format!("cardset.build_flow[{id}]"),
                            &cardset.build_flow,
                        )?,
                        compare_flow: compile_optional_flow(
                            &format!("cardset.compare_flow[{id}]"),
                            &cardset.compare_flow,
                        )?,
                        successors: cardset.successors.clone(),
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>, AppError>>()?;

        let comparison_flows = design
            .cardset_comparisons
            .iter()
            .map(|(id, comparison)| {
                Ok((
                    id.clone(),
                    RuntimeComparison {
                        cardset_a: comparison.cardset_a.clone(),
                        cardset_b: comparison.cardset_b.clone(),
                        compare_flow: compile_flow(
                            &format!("cardset_comparisons.compare_flow[{id}]"),
                            &comparison.compare_flow,
                        )?,
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>, AppError>>()?;

        Ok(RuntimeRule {
            name,
            player_count,
            description,
            design,
            match_flow,
            end_flow,
            cardset_flows,
            comparison_flows,
        })
    }

    pub fn start_session(
        room_code: String,
        runtime_rule: &RuntimeRule,
        player_ids: Vec<String>,
    ) -> Result<GameSession, AppError> {
        let players = build_players(runtime_rule, player_ids);
        let table = build_table(runtime_rule, &players);
        let deck = build_deck(runtime_rule)?;
        let hands = players
            .iter()
            .map(|player| (player.id.clone(), Vec::new()))
            .collect();

        let mut session = GameSession {
            id: uuid::Uuid::new_v4().to_string(),
            room_code,
            rule_name: runtime_rule.name.clone(),
            player_count: runtime_rule.player_count,
            status: "running".to_string(),
            active_flow: "match".to_string(),
            current_node: runtime_rule.match_flow.entry.clone(),
            players,
            table,
            deck,
            hands,
            discard_pile: Vec::new(),
            pending_action: None,
            settlement_results: HashMap::new(),
            execution_log: Vec::new(),
            last_successful_play: None,
            last_action_player_id: None,
            last_action_cards: Vec::new(),
            last_action_skipped: false,
        };

        Self::execute_until_blocked(runtime_rule, &mut session)?;
        Ok(session)
    }

    pub fn submit_action(
        runtime_rule: &RuntimeRule,
        session: &mut GameSession,
        player_id: &str,
        action: PlayerActionInput,
    ) -> Result<(), AppError> {
        let pending = session
            .pending_action
            .clone()
            .ok_or_else(|| AppError::InvalidInput("当前没有等待中的玩家动作".to_string()))?;

        if pending.player_id != player_id {
            return Err(AppError::InvalidInput("还没有轮到该玩家操作".to_string()));
        }

        match pending.component_type {
            21 => {
                handle_play_cards_action(runtime_rule, session, player_id, action)?;
            }
            22 => {
                let choice = action
                    .choice
                    .ok_or_else(|| AppError::InvalidInput("动作选择缺少 choice".to_string()))?;
                session.last_action_player_id = Some(player_id.to_string());
                session.last_action_cards.clear();
                session.last_action_skipped = false;
                session.table.insert("用户做出的选择".to_string(), choice);
                session
                    .execution_log
                    .push(format!("player {player_id} choose action {choice}"));
            }
            _ => {
                return Err(AppError::InvalidInput("未知的等待动作类型".to_string()));
            }
        }

        session.pending_action = None;
        session.current_node = next_target(&pending.id, &runtime_rule.match_flow, "next")?
            .unwrap_or_else(|| pending.id.clone());
        Self::execute_until_blocked(runtime_rule, session)
    }

    pub fn execute_until_blocked(
        runtime_rule: &RuntimeRule,
        session: &mut GameSession,
    ) -> Result<(), AppError> {
        for _ in 0..FLOW_STEP_LIMIT {
            if session.pending_action.is_some() || session.status == "finished" {
                return Ok(());
            }

            if session.active_flow == "end"
                && session.table.get("settlement_index").copied().unwrap_or(0)
                    >= session.players.len() as i64
            {
                session.status = "finished".to_string();
                return Ok(());
            }

            let flow = active_flow(runtime_rule, session)?;
            let node = flow
                .nodes
                .get(&session.current_node)
                .cloned()
                .ok_or_else(|| {
                    AppError::InvalidInput(format!("流程节点 {} 不存在", session.current_node))
                })?;

            let eval_ctx = RuntimeEvalContext {
                runtime_rule,
                flow,
                flow_kind: if session.active_flow == "match" {
                    FlowKind::Match
                } else {
                    FlowKind::End
                },
                compare: None,
                method: None,
                cardset_input: None,
                cardset_id: None,
                current_player_override: None,
            };

            session.execution_log.push(format!(
                "execute {} node {} type {}",
                session.active_flow, node.id, node.component_type
            ));

            match node.component_type {
                17 | 23 | 25 | 27 | 29 => move_to_next(session, flow, &node.id)?,
                4 => {
                    execute_assign(session, &eval_ctx, &node)?;
                    move_to_next(session, flow, &node.id)?;
                }
                16 => {
                    let condition = eval_condition(session, &eval_ctx, &node)?;
                    let label = if condition { "next_true" } else { "next_false" };
                    session.current_node =
                        next_target(&node.id, flow, label)?.ok_or_else(|| {
                            AppError::InvalidInput(format!("条件节点 {} 缺少 {}", node.id, label))
                        })?;
                }
                18 => {
                    session.active_flow = "end".to_string();
                    session.current_node = runtime_rule.end_flow.entry.clone();
                    session.table.insert("settlement_index".to_string(), 0);
                }
                19 => {
                    shuffle_deck(session);
                    move_to_next(session, flow, &node.id)?;
                }
                20 => {
                    execute_deal(session, &node)?;
                    move_to_next(session, flow, &node.id)?;
                }
                21 | 22 => {
                    let player_id = current_player_id(session, Some(&eval_ctx))?;
                    session.pending_action = Some(PendingAction {
                        id: node.id.clone(),
                        player_id,
                        component_type: node.component_type,
                        timer: content_u64(&node, "timer").unwrap_or(30),
                        options: node
                            .content
                            .as_ref()
                            .and_then(|content| content.get("options"))
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default(),
                    });
                    return Ok(());
                }
                24 => {
                    let result = content_i64(&node, "result").unwrap_or(0);
                    let player_id = current_player_id(session, Some(&eval_ctx))?;
                    session.settlement_results.insert(player_id, result);

                    let next_index = settlement_index(session) + 1;
                    session
                        .table
                        .insert("settlement_index".to_string(), next_index as i64);

                    if next_index >= session.players.len() {
                        session.status = "finished".to_string();
                        return Ok(());
                    }

                    session.current_node = runtime_rule.end_flow.entry.clone();
                }
                _ => move_to_next(session, flow, &node.id)?,
            }
        }

        Err(AppError::InvalidInput(
            "规则执行超过 1000 步仍未暂停或结束，可能存在死循环".to_string(),
        ))
    }
}

fn handle_play_cards_action(
    runtime_rule: &RuntimeRule,
    session: &mut GameSession,
    player_id: &str,
    action: PlayerActionInput,
) -> Result<(), AppError> {
    session.last_action_player_id = Some(player_id.to_string());

    if action.cards.is_empty() {
        if session.last_successful_play.is_none() {
            return Err(AppError::InvalidInput(
                "Cannot skip before any card has been played.".to_string(),
            ));
        }

        session.last_action_cards.clear();
        session.last_action_skipped = true;
        session
            .execution_log
            .push(format!("player {player_id} skipped"));
        return Ok(());
    }

    let hand_cards = session
        .hands
        .get(player_id)
        .cloned()
        .ok_or_else(|| AppError::InvalidInput("玩家手牌不存在".to_string()))?;
    let selected_cards = select_cards_from_hand(&hand_cards, &action.cards)?;
    let previous_round = session
        .last_successful_play
        .as_ref()
        .map(|play| resolve_play_by_cards(runtime_rule, play.cards.clone()))
        .transpose()?;
    let current_round = validate_and_resolve_play(
        runtime_rule,
        selected_cards.clone(),
        previous_round.as_ref(),
    )?;

    apply_play_cards(session, player_id, &current_round.cards)?;
    session.last_action_cards = current_round.cards.clone();
    session.last_action_skipped = false;
    session.last_successful_play = Some(LastSuccessfulPlay {
        player_id: player_id.to_string(),
        cards: current_round.cards.clone(),
    });
    session.execution_log.push(format!(
        "player {player_id} played {} as {}",
        current_round.cards.len(),
        current_round.cardset_name
    ));
    Ok(())
}

fn validate_and_resolve_play(
    runtime_rule: &RuntimeRule,
    selected_cards: Vec<GameCard>,
    previous_round: Option<&PlayResolution>,
) -> Result<PlayResolution, AppError> {
    if has_duplicate_card_ids(&selected_cards) {
        return Err(AppError::InvalidInput("不能重复选择同一张牌".to_string()));
    }

    let current_round = resolve_play_by_cards(runtime_rule, selected_cards)?;
    if let Some(previous_round) = previous_round {
        if !can_beat_previous_round(runtime_rule, &current_round, previous_round)? {
            return Err(AppError::InvalidInput(
                "出牌必须符合当前规则的牌型优先级关系".to_string(),
            ));
        }
    }

    Ok(current_round)
}

fn resolve_play_by_cards(
    runtime_rule: &RuntimeRule,
    cards: Vec<GameCard>,
) -> Result<PlayResolution, AppError> {
    let mut cardsets = runtime_rule.cardset_flows.values().collect::<Vec<_>>();
    cardsets.sort_by_key(|cardset| cardset.id.clone());

    for cardset in cardsets {
        let build_result = execute_cardset_build_flow(runtime_rule, cardset, &cards)?;
        if build_result.matched {
            return Ok(PlayResolution {
                cardset_id: cardset.id.clone(),
                cardset_name: cardset.name.clone(),
                cards: cards.clone(),
                properties: build_result.properties,
            });
        }
    }

    Err(AppError::InvalidInput(
        "不符合当前规则中的任何牌型".to_string(),
    ))
}

fn can_beat_previous_round(
    runtime_rule: &RuntimeRule,
    current_round: &PlayResolution,
    previous_round: &PlayResolution,
) -> Result<bool, AppError> {
    if current_round.cardset_id == previous_round.cardset_id {
        let current_cardset = runtime_rule
            .cardset_flows
            .get(&current_round.cardset_id)
            .ok_or_else(|| AppError::InvalidInput("当前牌型不存在".to_string()))?;

        return execute_same_cardset_compare(
            runtime_rule,
            current_cardset,
            current_round,
            previous_round,
        );
    }

    if let Some(result) =
        execute_cross_cardset_compare(runtime_rule, current_round, previous_round)?
    {
        return Ok(matches!(result, CompareFlowResult::A));
    }

    let current_cardset = runtime_rule
        .cardset_flows
        .get(&current_round.cardset_id)
        .ok_or_else(|| AppError::InvalidInput("当前牌型不存在".to_string()))?;
    Ok(current_cardset
        .successors
        .contains(&previous_round.cardset_id))
}

fn execute_same_cardset_compare(
    runtime_rule: &RuntimeRule,
    cardset: &RuntimeCardset,
    current_round: &PlayResolution,
    previous_round: &PlayResolution,
) -> Result<bool, AppError> {
    if cardset.compare_flow.nodes.is_empty() {
        return Ok(false);
    }

    let compare_ctx = CompareContext {
        cardset_a: to_cardset_runtime_result(current_round),
        cardset_b: to_cardset_runtime_result(previous_round),
    };
    let result = execute_compare_flow_result(runtime_rule, &cardset.compare_flow, &compare_ctx)?;
    Ok(matches!(result, CompareFlowResult::A))
}

fn execute_cross_cardset_compare(
    runtime_rule: &RuntimeRule,
    current_round: &PlayResolution,
    previous_round: &PlayResolution,
) -> Result<Option<CompareFlowResult>, AppError> {
    let current_cardset = runtime_rule
        .cardset_flows
        .get(&current_round.cardset_id)
        .ok_or_else(|| AppError::InvalidInput("当前牌型不存在".to_string()))?;
    let previous_cardset = runtime_rule
        .cardset_flows
        .get(&previous_round.cardset_id)
        .ok_or_else(|| AppError::InvalidInput("上一轮牌型不存在".to_string()))?;

    for comparison in runtime_rule.comparison_flows.values() {
        if comparison.cardset_a == current_cardset.name
            && comparison.cardset_b == previous_cardset.name
        {
            let compare_ctx = CompareContext {
                cardset_a: to_cardset_runtime_result(current_round),
                cardset_b: to_cardset_runtime_result(previous_round),
            };
            return execute_compare_flow_result(
                runtime_rule,
                &comparison.compare_flow,
                &compare_ctx,
            )
            .map(Some);
        }

        if comparison.cardset_a == previous_cardset.name
            && comparison.cardset_b == current_cardset.name
        {
            let compare_ctx = CompareContext {
                cardset_a: to_cardset_runtime_result(previous_round),
                cardset_b: to_cardset_runtime_result(current_round),
            };
            let result =
                execute_compare_flow_result(runtime_rule, &comparison.compare_flow, &compare_ctx)?;
            return Ok(Some(match result {
                CompareFlowResult::A => CompareFlowResult::B,
                CompareFlowResult::B => CompareFlowResult::A,
            }));
        }
    }

    Ok(None)
}

fn execute_cardset_build_flow(
    runtime_rule: &RuntimeRule,
    cardset: &RuntimeCardset,
    cards: &[GameCard],
) -> Result<CardsetBuildResult, AppError> {
    let mut current_node = cardset.build_flow.entry.clone();
    let mut visited = HashSet::new();

    for _ in 0..FLOW_STEP_LIMIT {
        if current_node.is_empty() {
            return Ok(CardsetBuildResult {
                matched: false,
                properties: HashMap::new(),
            });
        }

        let node = cardset
            .build_flow
            .nodes
            .get(&current_node)
            .cloned()
            .ok_or_else(|| {
                AppError::InvalidInput(format!("牌型构建节点 {} 不存在", current_node))
            })?;

        if !visited.insert(current_node.clone()) && node.component_type == 16 {
            return Err(AppError::InvalidInput("牌型构建流程存在死循环".to_string()));
        }

        let eval_ctx = RuntimeEvalContext {
            runtime_rule,
            flow: &cardset.build_flow,
            flow_kind: FlowKind::CardsetBuild,
            compare: None,
            method: None,
            cardset_input: Some(cards),
            cardset_id: Some(&cardset.id),
            current_player_override: None,
        };

        match node.component_type {
            27 => {
                current_node =
                    next_target(&node.id, &cardset.build_flow, "next")?.unwrap_or_default();
            }
            16 => {
                let branch = eval_condition_for_cardset(&eval_ctx, &node)?;
                current_node = next_target(
                    &node.id,
                    &cardset.build_flow,
                    if branch { "next_true" } else { "next_false" },
                )?
                .unwrap_or_default();
            }
            28 => {
                let content = node_content(&node)?;
                let matched = content.get("result").and_then(Value::as_i64).unwrap_or(0) == 1;
                let properties = if matched {
                    extract_int_properties(content.get("properties"))
                } else {
                    HashMap::new()
                };
                return Ok(CardsetBuildResult {
                    matched,
                    properties,
                });
            }
            _ => {
                let _ = eval_value_for_cardset(&eval_ctx, &node.id)?;
                current_node =
                    next_target(&node.id, &cardset.build_flow, "next")?.unwrap_or_default();
            }
        }
    }

    Err(AppError::InvalidInput("牌型构建流程执行超时".to_string()))
}

fn execute_compare_flow_result(
    runtime_rule: &RuntimeRule,
    flow: &RuntimeFlow,
    compare_ctx: &CompareContext,
) -> Result<CompareFlowResult, AppError> {
    let mut current_node = flow.entry.clone();
    let mut visited = HashSet::new();

    for _ in 0..FLOW_STEP_LIMIT {
        if current_node.is_empty() {
            return Err(AppError::InvalidInput("牌型比较流程未返回结果".to_string()));
        }

        let node = flow.nodes.get(&current_node).cloned().ok_or_else(|| {
            AppError::InvalidInput(format!("牌型比较节点 {} 不存在", current_node))
        })?;

        if !visited.insert(current_node.clone()) && node.component_type == 16 {
            return Err(AppError::InvalidInput("牌型比较流程存在死循环".to_string()));
        }

        let eval_ctx = RuntimeEvalContext {
            runtime_rule,
            flow,
            flow_kind: FlowKind::CardsetCompare,
            compare: Some(compare_ctx),
            method: None,
            cardset_input: None,
            cardset_id: None,
            current_player_override: None,
        };

        match node.component_type {
            29 => {
                current_node = next_target(&node.id, flow, "next")?.unwrap_or_default();
            }
            16 => {
                let branch = eval_condition_for_cardset(&eval_ctx, &node)?;
                current_node = next_target(
                    &node.id,
                    flow,
                    if branch { "next_true" } else { "next_false" },
                )?
                .unwrap_or_default();
            }
            30 => {
                let result = content_i64(&node, "result").unwrap_or(0);
                return Ok(if result == 0 {
                    CompareFlowResult::A
                } else {
                    CompareFlowResult::B
                });
            }
            _ => {
                let _ = eval_value_for_cardset(&eval_ctx, &node.id)?;
                current_node = next_target(&node.id, flow, "next")?.unwrap_or_default();
            }
        }
    }

    Err(AppError::InvalidInput("牌型比较流程执行超时".to_string()))
}

fn active_flow<'a>(
    runtime_rule: &'a RuntimeRule,
    session: &GameSession,
) -> Result<&'a RuntimeFlow, AppError> {
    match session.active_flow.as_str() {
        "match" => Ok(&runtime_rule.match_flow),
        "end" => Ok(&runtime_rule.end_flow),
        other => Err(AppError::InvalidInput(format!("未知流程：{other}"))),
    }
}

fn build_players(runtime_rule: &RuntimeRule, player_ids: Vec<String>) -> Vec<GamePlayer> {
    let defaults = runtime_rule
        .design
        .classes
        .get("player")
        .map(|class| merge_properties(&class.default_properties, &class.user_properties))
        .unwrap_or_default();

    player_ids
        .into_iter()
        .enumerate()
        .map(|(runtime_index, id)| GamePlayer {
            id,
            properties: defaults.clone(),
            runtime_index,
        })
        .collect()
}

fn build_table(runtime_rule: &RuntimeRule, players: &[GamePlayer]) -> HashMap<String, i64> {
    let mut table = runtime_rule
        .design
        .classes
        .get("table")
        .map(|class| merge_properties(&class.default_properties, &class.user_properties))
        .unwrap_or_default();

    table.insert("player_index".to_string(), 0);
    table.insert("index".to_string(), 0);
    table.insert("cur_max".to_string(), -1);
    table.insert("settlement_index".to_string(), 0);
    if let Some(first_player) = players.first() {
        table.insert(
            "本轮应出牌者".to_string(),
            resolve_player_numeric_id(first_player),
        );
    }
    table
}

fn merge_properties(
    defaults: &HashMap<String, RuleProperty>,
    users: &HashMap<String, RuleProperty>,
) -> HashMap<String, i64> {
    defaults
        .iter()
        .chain(users.iter())
        .map(|(name, property)| (name.clone(), property.default))
        .collect()
}

fn build_deck(runtime_rule: &RuntimeRule) -> Result<Vec<GameCard>, AppError> {
    let card_class = runtime_rule
        .design
        .classes
        .get("card")
        .ok_or_else(|| AppError::InvalidInput("规则缺少 card 类".to_string()))?;
    let properties = card_class
        .default_properties
        .iter()
        .chain(card_class.user_properties.iter())
        .collect::<Vec<_>>();

    let mut cards = vec![HashMap::new()];
    for (name, property) in properties {
        let values = enum_values(property).unwrap_or_else(|| vec![property.default]);
        let mut next_cards = Vec::new();
        for base in &cards {
            for value in &values {
                let mut card = base.clone();
                card.insert(name.clone(), *value);
                next_cards.push(card);
            }
        }
        cards = next_cards;
    }

    Ok(cards
        .into_iter()
        .enumerate()
        .map(|(index, properties)| GameCard {
            id: format!("card_{}", index + 1),
            properties,
        })
        .collect())
}

fn enum_values(property: &RuleProperty) -> Option<Vec<i64>> {
    property.config.as_ref()?.as_array().map(|items| {
        items
            .iter()
            .filter_map(|item| item.get("value").and_then(Value::as_i64))
            .collect()
    })
}

fn move_to_next(
    session: &mut GameSession,
    flow: &RuntimeFlow,
    node_id: &str,
) -> Result<(), AppError> {
    if let Some(target) = next_target(node_id, flow, "next")? {
        session.current_node = target;
        return Ok(());
    }

    Err(AppError::InvalidInput(format!(
        "节点 {node_id} 缺少后续节点"
    )))
}

fn next_target(node_id: &str, flow: &RuntimeFlow, label: &str) -> Result<Option<String>, AppError> {
    let node = flow
        .nodes
        .get(node_id)
        .ok_or_else(|| AppError::InvalidInput(format!("节点 {node_id} 不存在")))?;

    Ok(node
        .transitions
        .iter()
        .find(|transition| transition.label == label)
        .map(|transition| transition.target.clone()))
}

fn execute_assign(
    session: &mut GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    node: &RuntimeNode,
) -> Result<(), AppError> {
    let content = node_content(node)?;
    let component = content_string(content, "component")?;
    let rvalue = content_string(content, "rvalue")?;
    let value = eval_int(session, eval_ctx, rvalue)?;
    assign_property(session, eval_ctx, component, value)
}

fn execute_deal(session: &mut GameSession, node: &RuntimeNode) -> Result<(), AppError> {
    let count = node
        .count
        .as_ref()
        .and_then(Value::as_i64)
        .or_else(|| node.content.as_ref()?.get("count")?.as_i64())
        .unwrap_or(1)
        .max(0) as usize;
    let filters = node
        .content
        .as_ref()
        .and_then(|content| content.get("prop_pair"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let player_ids = session
        .players
        .iter()
        .map(|player| player.id.clone())
        .collect::<Vec<_>>();

    for _ in 0..count {
        for player_id in &player_ids {
            let Some(index) = session
                .deck
                .iter()
                .position(|card| card_matches_filters(card, &filters))
            else {
                return Err(AppError::InvalidInput(
                    "牌堆中没有符合条件的卡牌".to_string(),
                ));
            };

            let card = session.deck.remove(index);
            session
                .hands
                .entry(player_id.clone())
                .or_default()
                .push(card);
            update_player_hand_count(session, player_id);
        }
    }

    Ok(())
}

fn card_matches_filters(card: &GameCard, filters: &[Value]) -> bool {
    filters.iter().all(|filter| {
        let Some(name) = filter.get("prop_name").and_then(Value::as_str) else {
            return true;
        };
        let value = card.properties.get(name).copied().unwrap_or_default();
        let lower = filter
            .get("lower_bound")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MIN);
        let upper = filter
            .get("upper_bound")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX);
        value >= lower && value <= upper
    })
}

fn eval_condition(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    node: &RuntimeNode,
) -> Result<bool, AppError> {
    let content = node_content(node)?;
    let condition = content
        .get("condition")
        .and_then(Value::as_str)
        .unwrap_or("");

    if condition.is_empty() {
        return Ok(false);
    }

    truthy(eval_value(session, eval_ctx, condition)?)
}

fn eval_condition_for_cardset(
    eval_ctx: &RuntimeEvalContext<'_>,
    node: &RuntimeNode,
) -> Result<bool, AppError> {
    let dummy_session = empty_session_for_eval();
    let content = node_content(node)?;
    let condition = content
        .get("condition")
        .and_then(Value::as_str)
        .unwrap_or("");

    if condition.is_empty() {
        return Ok(false);
    }

    truthy(eval_value(&dummy_session, eval_ctx, condition)?)
}

fn eval_value_for_cardset(
    eval_ctx: &RuntimeEvalContext<'_>,
    node_id: &str,
) -> Result<EvalValue, AppError> {
    let dummy_session = empty_session_for_eval();
    eval_value(&dummy_session, eval_ctx, node_id)
}

fn truthy(value: EvalValue) -> Result<bool, AppError> {
    Ok(match value {
        EvalValue::Bool(value) => value,
        EvalValue::Int(value) => value != 0,
        EvalValue::Ints(values) => !values.is_empty(),
        EvalValue::Choice(value) => value != 0,
        EvalValue::Cards(cards) => !cards.is_empty(),
        EvalValue::Players(players) => !players.is_empty(),
        EvalValue::Cardset(cardset) => !cardset.cards.is_empty(),
        EvalValue::Table => true,
        EvalValue::None => false,
        EvalValue::Player(_) | EvalValue::PlayerIndex(_) | EvalValue::Card(_) => true,
    })
}

fn eval_int(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    node_id: &str,
) -> Result<i64, AppError> {
    match eval_value(session, eval_ctx, node_id)? {
        EvalValue::Int(value) => Ok(value),
        EvalValue::Ints(values) => Ok(values.first().copied().unwrap_or_default()),
        EvalValue::Bool(value) => Ok(i64::from(value)),
        EvalValue::Choice(value) => Ok(value),
        EvalValue::Table => Ok(0),
        EvalValue::PlayerIndex(index) => Ok(index as i64),
        EvalValue::Card(card) => Ok(primary_card_value(&card)),
        EvalValue::Cards(cards) => Ok(cards.len() as i64),
        EvalValue::Players(players) => Ok(players.len() as i64),
        EvalValue::Cardset(cardset) => Ok(cardset.cards.len() as i64),
        EvalValue::Player(player) => Ok(resolve_player_numeric_id(&player)),
        EvalValue::None => Ok(0),
    }
}

fn eval_value(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    node_id: &str,
) -> Result<EvalValue, AppError> {
    if node_id.trim().is_empty() {
        return Ok(EvalValue::None);
    }

    if let Some(value) = resolve_keyword_value(session, eval_ctx, node_id)? {
        return Ok(value);
    }

    let Some(node) = eval_ctx.flow.nodes.get(node_id) else {
        return Ok(parse_inline_value(node_id).unwrap_or(EvalValue::None));
    };
    let content = node_content(node)?;

    match node.component_type {
        1 => execute_method_call(session, eval_ctx, content),
        2 => execute_sort(session, eval_ctx, content),
        5 => {
            let selection = content_string(content, "selection")?;
            let index_ref = content_string(content, "index")?;
            let index = eval_int(session, eval_ctx, index_ref)?.max(0) as usize;

            match eval_value(session, eval_ctx, selection)? {
                EvalValue::Cards(cards) => Ok(cards
                    .get(index)
                    .cloned()
                    .map(EvalValue::Card)
                    .unwrap_or(EvalValue::None)),
                EvalValue::Players(players) => Ok(players
                    .get(index)
                    .cloned()
                    .map(EvalValue::Player)
                    .unwrap_or(EvalValue::None)),
                EvalValue::Cardset(cardset) => Ok(cardset
                    .cards
                    .get(index)
                    .cloned()
                    .map(EvalValue::Card)
                    .unwrap_or(EvalValue::None)),
                EvalValue::PlayerIndex(_) => Ok(session
                    .players
                    .get(index)
                    .cloned()
                    .map(EvalValue::Player)
                    .unwrap_or(EvalValue::None)),
                _ => Ok(EvalValue::None),
            }
        }
        6 => eval_property_access(session, eval_ctx, content),
        7 => {
            let selection = content_string(content, "selection")?;
            Ok(match eval_value(session, eval_ctx, selection)? {
                EvalValue::Cards(cards) => EvalValue::Int(cards.len() as i64),
                EvalValue::Players(players) => EvalValue::Int(players.len() as i64),
                EvalValue::Cardset(cardset) => EvalValue::Int(cardset.cards.len() as i64),
                _ => EvalValue::Int(0),
            })
        }
        8 | 9 => Ok(EvalValue::Int(
            content.get("value").and_then(Value::as_i64).unwrap_or(0),
        )),
        10 => {
            let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
            let left = eval_int(session, eval_ctx, content_string(content, "lval")?)?;
            let right = eval_int(session, eval_ctx, content_string(content, "rval")?)?;
            let value = match operator {
                1 => left - right,
                2 => left * right,
                3 => {
                    if right == 0 {
                        0
                    } else {
                        left / right
                    }
                }
                4 => {
                    if right == 0 {
                        0
                    } else {
                        left % right
                    }
                }
                _ => left + right,
            };
            Ok(EvalValue::Int(value))
        }
        11 => {
            let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
            let set_ref = content_string(content, "set")?;
            let component_ref = content_string(content, "component")?;
            let set_value = eval_value(session, eval_ctx, set_ref)?;
            let result = match set_value {
                EvalValue::Cards(cards) => evaluate_collection_logic_cards(
                    session,
                    eval_ctx,
                    operator,
                    &cards,
                    component_ref,
                )?,
                EvalValue::Players(players) => evaluate_collection_logic_players(
                    session,
                    eval_ctx,
                    operator,
                    &players,
                    component_ref,
                )?,
                _ => false,
            };
            Ok(EvalValue::Bool(result))
        }
        12 => {
            let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
            let left = truthy(eval_value(
                session,
                eval_ctx,
                content_string(content, "lval")?,
            )?)?;
            let right = truthy(eval_value(
                session,
                eval_ctx,
                content_string(content, "rval")?,
            )?)?;
            Ok(EvalValue::Bool(if operator == 0 {
                left && right
            } else {
                left || right
            }))
        }
        13 => Ok(EvalValue::Bool(!truthy(eval_value(
            session,
            eval_ctx,
            content_string(content, "component")?,
        )?)?)),
        14 => {
            let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
            let left = eval_scalar_value(session, eval_ctx, content_string(content, "lval")?)?;
            let right = eval_scalar_value(session, eval_ctx, content_string(content, "rval")?)?;
            Ok(EvalValue::Bool(compare_scalar_values(
                operator, left, right,
            )))
        }
        15 => {
            let card_set = content
                .get("card_set")
                .and_then(Value::as_str)
                .unwrap_or("");
            let card_rule = content
                .get("card_rule")
                .and_then(Value::as_str)
                .unwrap_or("");
            let cards = resolve_card_set_reference(session, eval_ctx, card_set)?;
            let result =
                match_cardset_by_rule_id(runtime_rule_from_ctx(eval_ctx), card_rule, cards)?;
            Ok(EvalValue::Bool(result.matched))
        }
        21 => Ok(EvalValue::Cards(session.last_action_cards.clone())),
        22 => Ok(EvalValue::Choice(table_i64(session, "用户做出的选择"))),
        27 => Ok(EvalValue::Cards(
            eval_ctx.cardset_input.unwrap_or(&[]).to_vec(),
        )),
        29 => Ok(EvalValue::None),
        30 => Ok(EvalValue::None),
        _ => Ok(EvalValue::None),
    }
}

fn eval_property_access(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    content: &Value,
) -> Result<EvalValue, AppError> {
    let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
    let ident = content.get("ident").and_then(Value::as_str).unwrap_or("");
    let property = content
        .get("property")
        .and_then(Value::as_str)
        .unwrap_or("");

    match operator {
        0 => resolve_object_property_access(session, eval_ctx, ident, property),
        1 => resolve_component_property_access(session, eval_ctx, ident, property),
        2 => resolve_collection_property_access(session, eval_ctx, ident, property),
        _ => Ok(EvalValue::None),
    }
}

fn resolve_object_property_access(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    ident: &str,
    property: &str,
) -> Result<EvalValue, AppError> {
    if ident == "table_0" {
        return access_table_property(session, eval_ctx, property);
    }

    if let Some(value) = resolve_object_reference(session, eval_ctx, ident)? {
        return access_property_from_value(property, value);
    }

    Ok(EvalValue::None)
}

fn resolve_component_property_access(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    ident: &str,
    property: &str,
) -> Result<EvalValue, AppError> {
    if let Some(node) = eval_ctx.flow.nodes.get(ident) {
        match node.component_type {
            17 => {
                if property == "牌桌" {
                    return Ok(EvalValue::Table);
                }
            }
            21 => {
                if property == "用户打出的牌组" {
                    return Ok(EvalValue::Cards(session.last_action_cards.clone()));
                }
            }
            22 => {
                if property == "用户做出的选择" {
                    return Ok(EvalValue::Choice(table_i64(session, "用户做出的选择")));
                }
            }
            23 => {
                if property == "玩家" {
                    let player_id = current_player_id(session, Some(eval_ctx))?;
                    let player = session
                        .players
                        .iter()
                        .find(|player| player.id == player_id)
                        .cloned()
                        .ok_or_else(|| AppError::InvalidInput("当前玩家不存在".to_string()))?;
                    return Ok(EvalValue::Player(player));
                }
            }
            25 => {
                if property == "调用该方法的对象" {
                    if let Some(method_ctx) = eval_ctx.method {
                        if let Some(value) =
                            resolve_object_reference(session, eval_ctx, &method_ctx.object_ref)?
                        {
                            return Ok(value);
                        }
                    }
                }
                if let Some(method_ctx) = eval_ctx.method {
                    if let Some(value) = method_ctx.parameters.get(property) {
                        return Ok(value.clone());
                    }
                }
            }
            27 => {
                if property == "传入牌组" {
                    return Ok(EvalValue::Cards(
                        eval_ctx.cardset_input.unwrap_or(&[]).to_vec(),
                    ));
                }
            }
            29 => {
                if let Some(compare) = eval_ctx.compare {
                    if property == "牌型 A" {
                        return Ok(EvalValue::Cardset(compare.cardset_a.clone()));
                    }
                    if property == "牌型 B" {
                        return Ok(EvalValue::Cardset(compare.cardset_b.clone()));
                    }
                    if let Some((prefix, field)) = property.split_once('.') {
                        let target = if prefix == "牌型 A" {
                            Some(&compare.cardset_a)
                        } else if prefix == "牌型 B" {
                            Some(&compare.cardset_b)
                        } else {
                            None
                        };
                        if let Some(target) = target {
                            return Ok(EvalValue::Int(
                                target.properties.get(field).copied().unwrap_or_default(),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let source = eval_value(session, eval_ctx, ident)?;
    access_property_from_value(property, source)
}

fn resolve_collection_property_access(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    ident: &str,
    property: &str,
) -> Result<EvalValue, AppError> {
    let source = eval_value(session, eval_ctx, ident)?;
    match source {
        EvalValue::Table => access_table_property(session, eval_ctx, property),
        EvalValue::Cards(cards) => Ok(EvalValue::Ints(
            cards
                .into_iter()
                .map(|card| {
                    scalar_from_value(access_property_from_value(property, EvalValue::Card(card))?)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        EvalValue::Players(players) => Ok(EvalValue::Ints(
            players
                .into_iter()
                .map(|player| {
                    scalar_from_value(access_property_from_value(
                        property,
                        EvalValue::Player(player),
                    )?)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        other => access_property_from_value(property, other),
    }
}

fn access_property_from_value(property: &str, source: EvalValue) -> Result<EvalValue, AppError> {
    match source {
        EvalValue::Card(card) => {
            if property == "id" {
                return Ok(EvalValue::Int(extract_numeric_suffix(&card.id)));
            }
            Ok(EvalValue::Int(
                card.properties.get(property).copied().unwrap_or_default(),
            ))
        }
        EvalValue::Player(player) => {
            if property == "id" {
                return Ok(EvalValue::Int(resolve_player_numeric_id(&player)));
            }
            Ok(EvalValue::Int(
                player.properties.get(property).copied().unwrap_or_default(),
            ))
        }
        EvalValue::Table => {
            if property == "玩家池" {
                return Ok(EvalValue::Players(Vec::new()));
            }
            if property == "卡牌池" {
                return Ok(EvalValue::Cards(Vec::new()));
            }
            Ok(EvalValue::Int(0))
        }
        EvalValue::Cards(cards) => {
            if property == "content" || property.is_empty() {
                return Ok(EvalValue::Int(cards.len() as i64));
            }
            Ok(EvalValue::Cards(cards))
        }
        EvalValue::Players(players) => {
            if property == "content" || property.is_empty() {
                return Ok(EvalValue::Int(players.len() as i64));
            }
            Ok(EvalValue::Players(players))
        }
        EvalValue::Cardset(cardset) => {
            if property == "cards" || property == "牌组" {
                return Ok(EvalValue::Cards(cardset.cards));
            }
            if property == "cardsetId" || property == "牌型 ID" {
                return Ok(EvalValue::Int(extract_numeric_suffix(&cardset.cardset_id)));
            }
            if property == "cardsetName" || property == "牌型名" {
                return Ok(EvalValue::Int(0));
            }
            if let Some((prefix, field)) = property.split_once('.') {
                if (prefix == "牌型 A" || prefix == "牌型 B") && !field.is_empty() {
                    return Ok(EvalValue::Int(
                        cardset.properties.get(field).copied().unwrap_or_default(),
                    ));
                }
            }
            Ok(EvalValue::Int(
                cardset
                    .properties
                    .get(property)
                    .copied()
                    .unwrap_or_default(),
            ))
        }
        EvalValue::Choice(choice) => Ok(if property == "用户做出的选择" {
            EvalValue::Choice(choice)
        } else {
            EvalValue::None
        }),
        EvalValue::Int(value) => Ok(EvalValue::Int(value)),
        EvalValue::Ints(values) => Ok(EvalValue::Ints(values)),
        EvalValue::Bool(value) => Ok(EvalValue::Bool(value)),
        EvalValue::PlayerIndex(index) => Ok(EvalValue::Int(index as i64)),
        EvalValue::None => Ok(EvalValue::None),
    }
}

fn scalar_from_value(value: EvalValue) -> Result<i64, AppError> {
    match value {
        EvalValue::Int(value) => Ok(value),
        EvalValue::Ints(values) => Ok(values.first().copied().unwrap_or_default()),
        EvalValue::Bool(value) => Ok(i64::from(value)),
        EvalValue::Choice(value) => Ok(value),
        EvalValue::Table => Ok(0),
        EvalValue::PlayerIndex(index) => Ok(index as i64),
        EvalValue::Card(card) => Ok(primary_card_value(&card)),
        EvalValue::Cards(cards) => Ok(cards.len() as i64),
        EvalValue::Players(players) => Ok(players.len() as i64),
        EvalValue::Cardset(cardset) => Ok(cardset.cards.len() as i64),
        EvalValue::Player(player) => Ok(resolve_player_numeric_id(&player)),
        EvalValue::None => Ok(0),
    }
}

fn assign_property(
    session: &mut GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    component_node_id: &str,
    value: i64,
) -> Result<(), AppError> {
    let node = eval_ctx.flow.nodes.get(component_node_id).ok_or_else(|| {
        AppError::InvalidInput(format!("赋值目标节点 {component_node_id} 不存在"))
    })?;

    if node.component_type != 6 {
        return Err(AppError::InvalidInput(
            "赋值目标必须是属性访问组件".to_string(),
        ));
    }

    let content = node_content(node)?;
    let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
    let ident = content.get("ident").and_then(Value::as_str).unwrap_or("");
    let property = content
        .get("property")
        .and_then(Value::as_str)
        .unwrap_or("");

    if operator != 0 {
        return Err(AppError::InvalidInput(
            "当前仅支持给对象属性赋值".to_string(),
        ));
    }

    if ident == "table_0" {
        session.table.insert(property.to_string(), value);
        return Ok(());
    }

    if let Some(index) = parse_player_object_index(ident) {
        if let Some(player) = session.players.get_mut(index) {
            player.properties.insert(property.to_string(), value);
            return Ok(());
        }
    }

    if update_card_property_by_object_ident(session, ident, property, value) {
        return Ok(());
    }

    Err(AppError::InvalidInput(format!("无法识别赋值对象 {ident}")))
}

fn apply_play_cards(
    session: &mut GameSession,
    player_id: &str,
    played_cards: &[GameCard],
) -> Result<(), AppError> {
    let hand = session
        .hands
        .get_mut(player_id)
        .ok_or_else(|| AppError::InvalidInput("玩家手牌不存在".to_string()))?;
    let mut removed = Vec::new();

    for played_card in played_cards {
        let Some(index) = hand.iter().position(|card| card.id == played_card.id) else {
            return Err(AppError::InvalidInput(format!(
                "玩家没有手牌 {}",
                played_card.id
            )));
        };
        removed.push(hand.remove(index));
    }

    session.discard_pile.extend(removed);
    update_player_hand_count(session, player_id);
    Ok(())
}

fn current_player_id(
    session: &GameSession,
    eval_ctx: Option<&RuntimeEvalContext<'_>>,
) -> Result<String, AppError> {
    if session.players.is_empty() {
        return Err(AppError::InvalidInput("对局中没有玩家".to_string()));
    }

    if let Some(override_id) = eval_ctx
        .and_then(|ctx| ctx.current_player_override.as_deref())
        .filter(|value| !value.is_empty())
    {
        return Ok(override_id.to_string());
    }

    let index = if session.active_flow == "end" {
        settlement_index(session)
    } else {
        normalize_player_index(session.players.len(), table_i64(session, "player_index"))
    };

    session
        .players
        .get(index)
        .map(|player| player.id.clone())
        .ok_or_else(|| AppError::InvalidInput("对局中没有玩家".to_string()))
}

fn settlement_index(session: &GameSession) -> usize {
    normalize_player_index(
        session.players.len(),
        session
            .table
            .get("settlement_index")
            .copied()
            .unwrap_or_default(),
    )
}

fn normalize_player_index(player_len: usize, raw_index: i64) -> usize {
    if player_len == 0 {
        return 0;
    }
    let raw = raw_index.max(0) as usize;
    if raw >= player_len {
        raw % player_len
    } else {
        raw
    }
}

fn update_player_hand_count(session: &mut GameSession, player_id: &str) {
    let count = session.hands.get(player_id).map(Vec::len).unwrap_or(0) as i64;
    if let Some(player) = session
        .players
        .iter_mut()
        .find(|player| player.id == player_id)
    {
        player.properties.insert("手牌数".to_string(), count);
    }
}

fn table_i64(session: &GameSession, property: &str) -> i64 {
    session.table.get(property).copied().unwrap_or_default()
}

fn access_table_property(
    session: &GameSession,
    _eval_ctx: &RuntimeEvalContext<'_>,
    property: &str,
) -> Result<EvalValue, AppError> {
    if property == "玩家池" {
        return Ok(EvalValue::Players(session.players.clone()));
    }

    if property == "卡牌池" {
        return Ok(EvalValue::Cards(session.deck.clone()));
    }

    Ok(EvalValue::Int(table_i64(session, property)))
}

fn node_content(node: &RuntimeNode) -> Result<&Value, AppError> {
    node.content
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput(format!("节点 {} 缺少 content", node.id)))
}

fn content_string<'a>(content: &'a Value, field: &str) -> Result<&'a str, AppError> {
    content
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidInput(format!("content 缺少字段 {field}")))
}

fn content_i64(node: &RuntimeNode, field: &str) -> Option<i64> {
    node.content.as_ref()?.get(field)?.as_i64()
}

fn content_u64(node: &RuntimeNode, field: &str) -> Option<u64> {
    node.content.as_ref()?.get(field)?.as_u64()
}

fn validate_metadata(name: &str, player_count: u8) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("规则名称不能为空".to_string()));
    }
    if player_count == 0 {
        return Err(AppError::InvalidInput("规则玩家人数必须大于 0".to_string()));
    }
    Ok(())
}

fn validate_design(design: &ExportedRuleDesign) -> Result<(), AppError> {
    for class_name in ["player", "card", "table"] {
        if !design.classes.contains_key(class_name) {
            return Err(AppError::InvalidInput(format!(
                "规则缺少固有类 classes.{class_name}"
            )));
        }
    }

    if design.cardsets.is_empty() {
        return Err(AppError::InvalidInput("规则至少需要一种牌型".to_string()));
    }

    validate_unique_names(
        "牌型名称",
        design
            .cardsets
            .values()
            .map(|cardset| cardset.name.as_str()),
    )?;

    for (class_name, class_def) in &design.classes {
        validate_property_map(
            &format!("类 {class_name} 默认属性"),
            &class_def.default_properties,
        )?;
        validate_property_map(
            &format!("类 {class_name} 用户属性"),
            &class_def.user_properties,
        )?;
        validate_unique_names("方法名", class_def.methods.keys().map(String::as_str))?;
        for (method_name, method) in &class_def.methods {
            validate_unique_names(
                &format!("方法 {method_name} 参数"),
                method.parameters.keys().map(String::as_str),
            )?;
            compile_flow(&format!("方法 {method_name} flow"), &method.flow)?;
        }
    }

    Ok(())
}

fn validate_property_map(
    scope: &str,
    properties: &HashMap<String, RuleProperty>,
) -> Result<(), AppError> {
    validate_unique_names(scope, properties.keys().map(String::as_str))?;
    for (name, property) in properties {
        if !matches!(property.data_type.as_str(), "int" | "enum") {
            return Err(AppError::InvalidInput(format!(
                "{scope}.{name} 的类型必须是 int 或 enum"
            )));
        }
    }
    Ok(())
}

fn validate_unique_names<'a>(
    scope: &str,
    names: impl Iterator<Item = &'a str>,
) -> Result<(), AppError> {
    let mut seen = HashSet::new();
    for name in names {
        if name.trim().is_empty() {
            return Err(AppError::InvalidInput(format!("{scope} 不能为空")));
        }
        if !seen.insert(name) {
            return Err(AppError::InvalidInput(format!(
                "{scope} 存在重复项：{name}"
            )));
        }
    }
    Ok(())
}

fn compile_optional_flow(name: &str, graph: &FlowGraph) -> Result<RuntimeFlow, AppError> {
    if graph.is_empty() {
        return Ok(RuntimeFlow {
            entry: String::new(),
            nodes: HashMap::new(),
        });
    }

    compile_flow(name, graph)
}

fn compile_flow(name: &str, graph: &FlowGraph) -> Result<RuntimeFlow, AppError> {
    if !graph.contains_key("1") {
        return Err(AppError::InvalidInput(format!(
            "{name} 缺少编号 1 的开始节点"
        )));
    }

    let mut nodes = HashMap::new();
    for (id, node) in graph {
        if !SUPPORTED_COMPONENT_TYPES.contains(&node.component_type) {
            return Err(AppError::InvalidInput(format!(
                "{name} 节点 {id} 使用了后端不支持的组件类型 {}",
                node.component_type
            )));
        }

        let transitions = collect_transitions(name, id, node, graph)?;
        nodes.insert(
            id.clone(),
            RuntimeNode {
                id: id.clone(),
                component_type: node.component_type,
                content: node.content.clone(),
                count: node.count.clone(),
                transitions,
            },
        );
    }

    Ok(RuntimeFlow {
        entry: "1".to_string(),
        nodes,
    })
}

fn collect_transitions(
    flow_name: &str,
    node_id: &str,
    node: &FlowNode,
    graph: &FlowGraph,
) -> Result<Vec<RuntimeTransition>, AppError> {
    let mut transitions = Vec::new();

    if let Some(next) = node.next.as_deref().filter(|value| !value.is_empty()) {
        ensure_target_exists(flow_name, node_id, "next", next, graph)?;
        transitions.push(RuntimeTransition {
            label: "next".to_string(),
            target: next.to_string(),
        });
    }

    if let Some(content) = node.content.as_ref() {
        for field in ["next", "next_true", "next_false"] {
            if let Some(target) = content
                .get(field)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                ensure_target_exists(flow_name, node_id, field, target, graph)?;
                transitions.push(RuntimeTransition {
                    label: field.to_string(),
                    target: target.to_string(),
                });
            }
        }
    }

    Ok(transitions)
}

fn ensure_target_exists(
    flow_name: &str,
    node_id: &str,
    field: &str,
    target: &str,
    graph: &FlowGraph,
) -> Result<(), AppError> {
    if graph.contains_key(target) {
        return Ok(());
    }

    Err(AppError::InvalidInput(format!(
        "{flow_name} 节点 {node_id} 的 {field} 指向不存在的节点 {target}"
    )))
}

fn runtime_rule_from_ctx<'a>(eval_ctx: &'a RuntimeEvalContext<'_>) -> &'a RuntimeRule {
    eval_ctx.runtime_rule
}

fn empty_session_for_eval() -> GameSession {
    GameSession {
        id: String::new(),
        room_code: String::new(),
        rule_name: String::new(),
        player_count: 0,
        status: "running".to_string(),
        active_flow: "match".to_string(),
        current_node: String::new(),
        players: Vec::new(),
        table: HashMap::new(),
        deck: Vec::new(),
        hands: HashMap::new(),
        discard_pile: Vec::new(),
        pending_action: None,
        settlement_results: HashMap::new(),
        execution_log: Vec::new(),
        last_successful_play: None,
        last_action_player_id: None,
        last_action_cards: Vec::new(),
        last_action_skipped: false,
    }
}

fn parse_inline_value(node_id: &str) -> Option<EvalValue> {
    let normalized = node_id.trim();
    if normalized.eq_ignore_ascii_case("true") {
        return Some(EvalValue::Bool(true));
    }
    if normalized.eq_ignore_ascii_case("false") {
        return Some(EvalValue::Bool(false));
    }
    normalized.parse::<i64>().ok().map(EvalValue::Int)
}

fn resolve_keyword_value(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    raw: &str,
) -> Result<Option<EvalValue>, AppError> {
    let normalized = raw.trim();

    if matches!(
        normalized,
        "cards" | "card_set" | "selectedCards" | "selected_cards" | "初始牌组" | "牌组"
    ) {
        return Ok(Some(EvalValue::Cards(
            eval_ctx.cardset_input.unwrap_or(&[]).to_vec(),
        )));
    }

    if matches!(
        normalized,
        "A" | "a" | "cardsetA" | "cardset_a" | "currentRound" | "current_round" | "牌型 A"
    ) {
        return Ok(eval_ctx
            .compare
            .map(|compare| EvalValue::Cardset(compare.cardset_a.clone())));
    }

    if matches!(
        normalized,
        "B" | "b" | "cardsetB" | "cardset_b" | "previousRound" | "previous_round" | "牌型 B"
    ) {
        return Ok(eval_ctx
            .compare
            .map(|compare| EvalValue::Cardset(compare.cardset_b.clone())));
    }

    if normalized == "玩家" {
        let player_id = current_player_id(session, Some(eval_ctx))?;
        let player = session
            .players
            .iter()
            .find(|player| player.id == player_id)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("当前玩家不存在".to_string()))?;
        return Ok(Some(EvalValue::Player(player)));
    }

    if let Some(method_ctx) = eval_ctx.method {
        if let Some(value) = method_ctx.parameters.get(normalized) {
            return Ok(Some(value.clone()));
        }
        if normalized == "调用该方法的对象" {
            if let Some(value) =
                resolve_object_reference(session, eval_ctx, &method_ctx.object_ref)?
            {
                return Ok(Some(value));
            }
        }
    }

    Ok(None)
}

fn resolve_object_reference(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    ident: &str,
) -> Result<Option<EvalValue>, AppError> {
    if ident == "table_0" {
        return Ok(Some(EvalValue::Table));
    }

    if ident == "玩家" {
        let player_id = current_player_id(session, Some(eval_ctx))?;
        let player = session
            .players
            .iter()
            .find(|player| player.id == player_id)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("当前玩家不存在".to_string()))?;
        return Ok(Some(EvalValue::Player(player)));
    }

    if let Some(index) = parse_player_object_index(ident) {
        return Ok(session.players.get(index).cloned().map(EvalValue::Player));
    }

    if let Some(card) = find_card_by_object_ident(session, eval_ctx, ident) {
        return Ok(Some(EvalValue::Card(card)));
    }

    Ok(None)
}

fn execute_sort(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    content: &Value,
) -> Result<EvalValue, AppError> {
    let selection = content_string(content, "selection")?;
    let properties = content
        .get("properties")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    match eval_value(session, eval_ctx, selection)? {
        EvalValue::Cards(mut cards) => {
            cards.sort_by(|left, right| compare_card_by_sort_properties(left, right, &properties));
            Ok(EvalValue::Cards(cards))
        }
        EvalValue::Players(mut players) => {
            players
                .sort_by(|left, right| compare_player_by_sort_properties(left, right, &properties));
            Ok(EvalValue::Players(players))
        }
        other => Ok(other),
    }
}

fn compare_card_by_sort_properties(
    left: &GameCard,
    right: &GameCard,
    properties: &[Value],
) -> Ordering {
    for property in properties {
        let name = property.get("name").and_then(Value::as_str).unwrap_or("");
        let order = property.get("order").and_then(Value::as_i64).unwrap_or(1);
        let left_value = left.properties.get(name).copied().unwrap_or_default();
        let right_value = right.properties.get(name).copied().unwrap_or_default();
        let cmp = left_value.cmp(&right_value);
        if cmp != Ordering::Equal {
            return if order == 0 { cmp.reverse() } else { cmp };
        }
    }
    Ordering::Equal
}

fn compare_player_by_sort_properties(
    left: &GamePlayer,
    right: &GamePlayer,
    properties: &[Value],
) -> Ordering {
    for property in properties {
        let name = property.get("name").and_then(Value::as_str).unwrap_or("");
        let order = property.get("order").and_then(Value::as_i64).unwrap_or(1);
        let left_value = left.properties.get(name).copied().unwrap_or_default();
        let right_value = right.properties.get(name).copied().unwrap_or_default();
        let cmp = left_value.cmp(&right_value);
        if cmp != Ordering::Equal {
            return if order == 0 { cmp.reverse() } else { cmp };
        }
    }
    Ordering::Equal
}

fn evaluate_collection_logic_cards(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    operator: i64,
    cards: &[GameCard],
    component_ref: &str,
) -> Result<bool, AppError> {
    let mut matched_any = false;

    for card in cards {
        let mut nested_session = session.clone();
        nested_session.last_action_cards = vec![card.clone()];
        let nested_ctx = RuntimeEvalContext {
            runtime_rule: eval_ctx.runtime_rule,
            flow: eval_ctx.flow,
            flow_kind: eval_ctx.flow_kind,
            compare: eval_ctx.compare,
            method: eval_ctx.method,
            cardset_input: Some(std::slice::from_ref(card)),
            cardset_id: eval_ctx.cardset_id,
            current_player_override: eval_ctx.current_player_override.clone(),
        };
        let result = truthy(eval_value(&nested_session, &nested_ctx, component_ref)?)?;
        if result {
            matched_any = true;
            if operator == 0 {
                return Ok(true);
            }
        } else if operator == 1 {
            return Ok(false);
        }
    }

    Ok(if operator == 0 {
        matched_any
    } else {
        !cards.is_empty()
    })
}

fn evaluate_collection_logic_players(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    operator: i64,
    players: &[GamePlayer],
    component_ref: &str,
) -> Result<bool, AppError> {
    let mut matched_any = false;

    for player in players {
        let nested_ctx = RuntimeEvalContext {
            runtime_rule: eval_ctx.runtime_rule,
            flow: eval_ctx.flow,
            flow_kind: eval_ctx.flow_kind,
            compare: eval_ctx.compare,
            method: eval_ctx.method,
            cardset_input: eval_ctx.cardset_input,
            cardset_id: eval_ctx.cardset_id,
            current_player_override: Some(player.id.clone()),
        };
        let mut nested_session = session.clone();
        nested_session.table.insert(
            "settlement_index".to_string(),
            nested_session
                .players
                .iter()
                .position(|item| item.id == player.id)
                .unwrap_or_default() as i64,
        );
        let result = truthy(eval_value(&nested_session, &nested_ctx, component_ref)?)?;
        if result {
            matched_any = true;
            if operator == 0 {
                return Ok(true);
            }
        } else if operator == 1 {
            return Ok(false);
        }
    }

    Ok(if operator == 0 {
        matched_any
    } else {
        !players.is_empty()
    })
}

fn eval_scalar_value(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    node_id: &str,
) -> Result<ScalarValue, AppError> {
    match eval_value(session, eval_ctx, node_id)? {
        EvalValue::Int(value) => Ok(ScalarValue::Int(value)),
        EvalValue::Ints(values) => Ok(ScalarValue::Int(
            values.first().copied().unwrap_or_default(),
        )),
        EvalValue::Bool(value) => Ok(ScalarValue::Bool(value)),
        EvalValue::Choice(value) => Ok(ScalarValue::Int(value)),
        EvalValue::Table => Ok(ScalarValue::Int(0)),
        EvalValue::PlayerIndex(value) => Ok(ScalarValue::Int(value as i64)),
        EvalValue::Card(card) => Ok(ScalarValue::Int(primary_card_value(&card))),
        EvalValue::Player(player) => Ok(ScalarValue::Int(resolve_player_numeric_id(&player))),
        EvalValue::None => Ok(ScalarValue::Int(0)),
        EvalValue::Cards(cards) => Ok(ScalarValue::Int(cards.len() as i64)),
        EvalValue::Players(players) => Ok(ScalarValue::Int(players.len() as i64)),
        EvalValue::Cardset(cardset) => Ok(ScalarValue::Int(cardset.cards.len() as i64)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScalarValue {
    Int(i64),
    Bool(bool),
}

fn compare_scalar_values(operator: i64, left: ScalarValue, right: ScalarValue) -> bool {
    match (left, right) {
        (ScalarValue::Bool(left), ScalarValue::Bool(right)) => match operator {
            0 => left == right,
            1 => left && !right,
            2 => !left && right,
            3 => i64::from(left) >= i64::from(right),
            4 => i64::from(left) <= i64::from(right),
            _ => false,
        },
        (ScalarValue::Int(left), ScalarValue::Int(right)) => match operator {
            0 => left == right,
            1 => left > right,
            2 => left < right,
            3 => left >= right,
            4 => left <= right,
            _ => false,
        },
        (left, right) => compare_scalar_values(operator, scalar_to_int(left), scalar_to_int(right)),
    }
}

fn scalar_to_int(value: ScalarValue) -> ScalarValue {
    match value {
        ScalarValue::Bool(value) => ScalarValue::Int(i64::from(value)),
        other => other,
    }
}

fn resolve_card_set_reference(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    raw: &str,
) -> Result<Vec<GameCard>, AppError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    if raw.contains('|') {
        let mut cards = Vec::new();
        for card_id in raw
            .split('|')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let card = session
                .deck
                .iter()
                .chain(session.discard_pile.iter())
                .chain(session.hands.values().flatten())
                .find(|card| card.id == card_id)
                .cloned()
                .or_else(|| {
                    eval_ctx
                        .cardset_input
                        .and_then(|items| items.iter().find(|card| card.id == card_id).cloned())
                })
                .ok_or_else(|| AppError::InvalidInput(format!("牌组中不存在卡牌 {card_id}")))?;
            cards.push(card);
        }
        return Ok(cards);
    }

    match eval_value(session, eval_ctx, raw)? {
        EvalValue::Cards(cards) => Ok(cards),
        EvalValue::Card(card) => Ok(vec![card]),
        EvalValue::Cardset(cardset) => Ok(cardset.cards),
        _ => Ok(Vec::new()),
    }
}

fn match_cardset_by_rule_id(
    runtime_rule: &RuntimeRule,
    card_rule: &str,
    cards: Vec<GameCard>,
) -> Result<CardsetBuildResult, AppError> {
    let cardset = runtime_rule
        .cardset_flows
        .get(card_rule)
        .ok_or_else(|| AppError::InvalidInput(format!("牌型 {card_rule} 不存在")))?;
    execute_cardset_build_flow(runtime_rule, cardset, &cards)
}

fn execute_method_call(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    content: &Value,
) -> Result<EvalValue, AppError> {
    let object_ref = content.get("object").and_then(Value::as_str).unwrap_or("");
    let method_name = content.get("method").and_then(Value::as_str).unwrap_or("");
    if object_ref.is_empty() || method_name.is_empty() {
        return Ok(EvalValue::None);
    }

    let class_name = resolve_class_name_by_object_ref(object_ref)?;
    let method = eval_ctx
        .runtime_rule
        .design
        .classes
        .get(class_name)
        .and_then(|class| class.methods.get(method_name))
        .ok_or_else(|| AppError::InvalidInput(format!("方法 {class_name}.{method_name} 不存在")))?;
    let method_flow = compile_flow(&format!("method:{class_name}.{method_name}"), &method.flow)?;

    let parameter_values = content
        .get("parameter")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut parameters = HashMap::new();
    for ((param_name, _), raw_value) in method.parameters.iter().zip(parameter_values.iter()) {
        let value = if let Some(raw) = raw_value.as_str() {
            eval_value(session, eval_ctx, raw)?
        } else if let Some(int_value) = raw_value.as_i64() {
            EvalValue::Int(int_value)
        } else {
            EvalValue::None
        };
        parameters.insert(param_name.clone(), value);
    }

    execute_method_flow(
        session,
        eval_ctx.runtime_rule,
        &method_flow,
        MethodContext {
            object_ref: object_ref.to_string(),
            parameters,
        },
        eval_ctx.cardset_input,
        eval_ctx.cardset_id,
        eval_ctx.current_player_override.clone(),
    )
}

fn execute_method_flow(
    session: &GameSession,
    runtime_rule: &RuntimeRule,
    flow: &RuntimeFlow,
    method_ctx: MethodContext,
    cardset_input: Option<&[GameCard]>,
    cardset_id: Option<&str>,
    current_player_override: Option<String>,
) -> Result<EvalValue, AppError> {
    let mut current_node = flow.entry.clone();
    let mut visited = HashSet::new();

    for _ in 0..FLOW_STEP_LIMIT {
        if current_node.is_empty() {
            return Ok(EvalValue::None);
        }

        let node =
            flow.nodes.get(&current_node).cloned().ok_or_else(|| {
                AppError::InvalidInput(format!("方法节点 {} 不存在", current_node))
            })?;

        if !visited.insert(current_node.clone()) && node.component_type == 16 {
            return Err(AppError::InvalidInput("方法流程存在死循环".to_string()));
        }

        let eval_ctx = RuntimeEvalContext {
            runtime_rule,
            flow,
            flow_kind: FlowKind::Method,
            compare: None,
            method: Some(&method_ctx),
            cardset_input,
            cardset_id,
            current_player_override: current_player_override.clone(),
        };

        match node.component_type {
            25 => {
                current_node = next_target(&node.id, flow, "next")?.unwrap_or_default();
            }
            16 => {
                let branch = eval_condition(session, &eval_ctx, &node)?;
                current_node = next_target(
                    &node.id,
                    flow,
                    if branch { "next_true" } else { "next_false" },
                )?
                .unwrap_or_default();
            }
            26 => {
                let content = node_content(&node)?;
                let return_ref = content.get("return").and_then(Value::as_str).unwrap_or("");
                if return_ref == "void" || return_ref.is_empty() {
                    return Ok(EvalValue::None);
                }
                return eval_value(session, &eval_ctx, return_ref);
            }
            _ => {
                let _ = eval_value(session, &eval_ctx, &node.id)?;
                current_node = next_target(&node.id, flow, "next")?.unwrap_or_default();
            }
        }
    }

    Err(AppError::InvalidInput("方法流程执行超时".to_string()))
}

fn resolve_class_name_by_object_ref(object_ref: &str) -> Result<&'static str, AppError> {
    if object_ref == "table_0" {
        return Ok("table");
    }
    if object_ref.starts_with("player_") {
        return Ok("player");
    }
    if object_ref.starts_with("card_") {
        return Ok("card");
    }
    Err(AppError::InvalidInput(format!("无法识别对象 {object_ref}")))
}

fn has_duplicate_card_ids(cards: &[GameCard]) -> bool {
    let mut seen = HashSet::new();
    cards.iter().any(|card| !seen.insert(card.id.clone()))
}

fn select_cards_from_hand(
    hand_cards: &[GameCard],
    card_ids: &[String],
) -> Result<Vec<GameCard>, AppError> {
    let mut selected = Vec::new();
    for card_id in card_ids {
        let card = hand_cards
            .iter()
            .find(|card| &card.id == card_id)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput(format!("所选牌不在当前手牌中：{card_id}")))?;
        selected.push(card);
    }
    Ok(selected)
}

fn to_cardset_runtime_result(play: &PlayResolution) -> CardsetRuntimeResult {
    CardsetRuntimeResult {
        cardset_id: play.cardset_id.clone(),
        cardset_name: play.cardset_name.clone(),
        cards: play.cards.clone(),
        properties: play.properties.clone(),
    }
}

fn extract_int_properties(value: Option<&Value>) -> HashMap<String, i64> {
    let Some(Value::Object(object)) = value else {
        return HashMap::new();
    };

    object
        .iter()
        .filter_map(|(key, value)| value.as_i64().map(|value| (key.clone(), value)))
        .collect()
}

fn extract_numeric_suffix(value: &str) -> i64 {
    value
        .rsplit('_')
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .unwrap_or_default()
}

fn resolve_player_numeric_id(player: &GamePlayer) -> i64 {
    player.runtime_index as i64
}

fn parse_player_object_index(ident: &str) -> Option<usize> {
    ident
        .strip_prefix("player_")
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| ident.parse::<usize>().ok())
}

fn parse_card_object_index(ident: &str) -> Option<usize> {
    ident
        .strip_prefix("card_")
        .and_then(|value| value.parse::<usize>().ok())
}

fn find_card_by_object_ident(
    session: &GameSession,
    eval_ctx: &RuntimeEvalContext<'_>,
    ident: &str,
) -> Option<GameCard> {
    if let Some(index) = parse_card_object_index(ident) {
        let expected_id = format!("card_{index}");
        if let Some(card) = eval_ctx
            .cardset_input
            .and_then(|cards| cards.iter().find(|card| card.id == expected_id).cloned())
        {
            return Some(card);
        }

        return session
            .deck
            .iter()
            .chain(session.discard_pile.iter())
            .chain(session.hands.values().flatten())
            .find(|card| card.id == expected_id)
            .cloned();
    }

    None
}

fn update_card_property_by_object_ident(
    session: &mut GameSession,
    ident: &str,
    property: &str,
    value: i64,
) -> bool {
    let Some(index) = parse_card_object_index(ident) else {
        return false;
    };
    let expected_id = format!("card_{index}");

    for card in &mut session.deck {
        if card.id == expected_id {
            card.properties.insert(property.to_string(), value);
            return true;
        }
    }

    for card in &mut session.discard_pile {
        if card.id == expected_id {
            card.properties.insert(property.to_string(), value);
            return true;
        }
    }

    for hand in session.hands.values_mut() {
        for card in hand {
            if card.id == expected_id {
                card.properties.insert(property.to_string(), value);
                return true;
            }
        }
    }

    false
}

fn primary_card_value(card: &GameCard) -> i64 {
    card.properties
        .get("点数")
        .copied()
        .or_else(|| card.properties.get("鐐规暟").copied())
        .or_else(|| card.properties.values().next().copied())
        .unwrap_or_default()
}

fn shuffle_deck(session: &mut GameSession) {
    session.deck.reverse();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_tiny_demo_rule() -> RuntimeRule {
        let content = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test2.json"),
        )
        .expect("test2.json should exist");
        let design: ExportedRuleDesign =
            serde_json::from_str(&content).expect("test2.json should be valid rule json");

        RuleEngine::parse(
            "Tiny Demo".to_string(),
            2,
            "engine execution regression".to_string(),
            design,
        )
        .expect("tiny demo should compile")
    }

    #[test]
    fn tiny_demo_rule_runs_from_start_to_settlement() {
        let runtime_rule = load_tiny_demo_rule();
        let mut session = RuleEngine::start_session(
            "room-test".to_string(),
            &runtime_rule,
            vec!["player-a".to_string(), "player-b".to_string()],
        )
        .expect("session should start");

        let pending = session
            .pending_action
            .clone()
            .expect("rule should wait for player card action");
        assert_eq!(pending.component_type, 21);
        assert_eq!(pending.player_id, "player-a");

        let first_card = session
            .hands
            .get("player-a")
            .and_then(|cards| cards.first())
            .map(|card| card.id.clone())
            .expect("player-a should have a dealt card");

        RuleEngine::submit_action(
            &runtime_rule,
            &mut session,
            "player-a",
            PlayerActionInput {
                cards: vec![first_card],
                choice: None,
            },
        )
        .expect("valid single-card play should execute");

        assert_eq!(session.status, "finished");
        assert!(session.pending_action.is_none());
        assert_eq!(session.settlement_results.get("player-a"), Some(&1));
        assert_eq!(session.settlement_results.get("player-b"), Some(&0));
    }

    fn load_war_rule() -> RuntimeRule {
        let content = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("war.json"),
        )
        .expect("war.json should exist");
        let design: ExportedRuleDesign =
            serde_json::from_str(&content).expect("war.json should be valid rule json");

        RuleEngine::parse(
            "War 拼点战争".to_string(),
            2,
            "5 轮翻牌比大小".to_string(),
            design,
        )
        .expect("war rule should compile")
    }

    fn pick_card_id(session: &GameSession, player_id: &str, hand_index: usize) -> String {
        session
            .hands
            .get(player_id)
            .and_then(|cards| cards.get(hand_index))
            .map(|card| card.id.clone())
            .unwrap_or_else(|| panic!("player {player_id} should have card at index {hand_index}"))
    }

    fn card_point(session: &GameSession, player_id: &str, card_id: &str) -> i64 {
        session
            .hands
            .get(player_id)
            .and_then(|cards| cards.iter().find(|card| card.id == card_id))
            .and_then(|card| card.properties.get("点数").copied())
            .unwrap_or_default()
    }

    #[test]
    fn simulate_war_full_match() {
        let runtime_rule = load_war_rule();
        let player_ids = vec!["player-a".to_string(), "player-b".to_string()];
        let mut session =
            RuleEngine::start_session("room-war".to_string(), &runtime_rule, player_ids.clone())
                .expect("war session should start");

        // 每人开局发到 5 张牌。
        assert_eq!(session.hands.get("player-a").map(Vec::len), Some(5));
        assert_eq!(session.hands.get("player-b").map(Vec::len), Some(5));

        for round in 0..5 {
            // 轮到玩家 A 出牌。
            let pending = session
                .pending_action
                .clone()
                .unwrap_or_else(|| panic!("round {round}: expected player A pending action"));
            assert_eq!(pending.component_type, 21);
            assert_eq!(
                pending.player_id, "player-a",
                "round {round}: player A should act first"
            );

            let p0_card_id = pick_card_id(&session, "player-a", 0);
            let p0_point = card_point(&session, "player-a", &p0_card_id);
            RuleEngine::submit_action(
                &runtime_rule,
                &mut session,
                "player-a",
                PlayerActionInput {
                    cards: vec![p0_card_id],
                    choice: None,
                },
            )
            .unwrap_or_else(|err| panic!("round {round}: player A play failed: {err:?}"));

            // 轮到玩家 B 出牌。
            let pending = session
                .pending_action
                .clone()
                .unwrap_or_else(|| panic!("round {round}: expected player B pending action"));
            assert_eq!(
                pending.player_id, "player-b",
                "round {round}: player B should act second"
            );

            let p1_card_id = pick_card_id(&session, "player-b", 0);
            let p1_point = card_point(&session, "player-b", &p1_card_id);
            RuleEngine::submit_action(
                &runtime_rule,
                &mut session,
                "player-b",
                PlayerActionInput {
                    cards: vec![p1_card_id],
                    choice: None,
                },
            )
            .unwrap_or_else(|err| panic!("round {round}: player B play failed: {err:?}"));

            // 比对后引擎应该按比较结果累加 round_wins。
            let p0_wins = session
                .players
                .iter()
                .find(|player| player.id == "player-a")
                .and_then(|player| player.properties.get("round_wins").copied())
                .unwrap_or_default();
            let p1_wins = session
                .players
                .iter()
                .find(|player| player.id == "player-b")
                .and_then(|player| player.properties.get("round_wins").copied())
                .unwrap_or_default();

            match p0_point.cmp(&p1_point) {
                std::cmp::Ordering::Greater => {
                    assert!(
                        p0_wins >= 1,
                        "round {round}: player A point {p0_point} > {p1_point} should add a win"
                    );
                }
                std::cmp::Ordering::Less => {
                    assert!(
                        p1_wins >= 1,
                        "round {round}: player B point {p1_point} > {p0_point} should add a win"
                    );
                }
                std::cmp::Ordering::Equal => {
                    // 平局两侧都不加分。
                }
            }
        }

        // 五轮跑完后应进入结算并产出胜者（除非完全平局）。
        assert_eq!(session.status, "finished", "session should be finished");
        assert!(session.pending_action.is_none());
        assert_eq!(session.discard_pile.len(), 10);

        let p0_result = session
            .settlement_results
            .get("player-a")
            .copied()
            .unwrap_or_default();
        let p1_result = session
            .settlement_results
            .get("player-b")
            .copied()
            .unwrap_or_default();
        let p0_wins = session
            .players
            .iter()
            .find(|player| player.id == "player-a")
            .and_then(|player| player.properties.get("round_wins").copied())
            .unwrap_or_default();
        let p1_wins = session
            .players
            .iter()
            .find(|player| player.id == "player-b")
            .and_then(|player| player.properties.get("round_wins").copied())
            .unwrap_or_default();

        match p0_wins.cmp(&p1_wins) {
            std::cmp::Ordering::Greater => {
                assert_eq!(p0_result, 1, "player A wins more rounds, should win match");
                assert_eq!(p1_result, 0);
            }
            std::cmp::Ordering::Less => {
                assert_eq!(p1_result, 1, "player B wins more rounds, should win match");
                assert_eq!(p0_result, 0);
            }
            std::cmp::Ordering::Equal => {
                // 局合：双方 round_wins 相同时 winner_index 设为 -1，两侧 result=0。
                assert_eq!(p0_result, 0);
                assert_eq!(p1_result, 0);
            }
        }
    }
}
