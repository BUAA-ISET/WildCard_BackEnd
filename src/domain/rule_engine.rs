use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

const SUPPORTED_COMPONENT_TYPES: &[u16] = &[
    1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
    28, 29, 30,
];

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
    #[serde(default)]
    pub cards: Vec<String>,
    #[serde(default)]
    pub choice: Option<i64>,
}

#[derive(Debug, Clone)]
enum EvalValue {
    Int(i64),
    Bool(bool),
    PlayerIndex(usize),
    Card(GameCard),
    Cards(Vec<GameCard>),
    None,
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

        let match_flow = compile_flow("对局流程", &design.match_flow)?;
        let end_flow = compile_flow("结算流程", &design.end_flow)?;
        let cardset_flows = design
            .cardsets
            .iter()
            .map(|(id, cardset)| {
                Ok((
                    id.clone(),
                    RuntimeCardset {
                        name: cardset.name.clone(),
                        build_flow: compile_flow(
                            &format!("牌型「{}」构建流程", cardset.name),
                            &cardset.build_flow,
                        )?,
                        compare_flow: compile_optional_flow(
                            &format!("牌型「{}」兼容比较流程", cardset.name),
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
                            &format!(
                                "牌型比较「{}-{}」流程",
                                comparison.cardset_a, comparison.cardset_b
                            ),
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
        let table = build_table(runtime_rule);
        let deck = build_deck(runtime_rule)?;
        let hands = players
            .iter()
            .map(|player| (player.id.clone(), Vec::new()))
            .collect();

        // 规则文档要求出牌组件维护“上次成功出牌”和“出牌玩家”，开局时先初始化为空。
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
            21 => apply_play_cards(session, player_id, action.cards)?,
            22 => {
                let choice = action
                    .choice
                    .ok_or_else(|| AppError::InvalidInput("动作选择缺少 choice".to_string()))?;
                session
                    .execution_log
                    .push(format!("玩家 {player_id} 选择动作 {choice}"));
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
        // 防御用户规则中的死循环。真正的规则循环应该在出牌/动作组件处暂停等待玩家输入。
        for _ in 0..1000 {
            if session.status != "running" || session.pending_action.is_some() {
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

            session.execution_log.push(format!(
                "执行 {} 流程节点 {}，组件 {}",
                session.active_flow, node.id, node.component_type
            ));

            match node.component_type {
                17 | 23 => move_to_next(session, flow, &node.id)?,
                4 => {
                    execute_assign(session, flow, &node)?;
                    move_to_next(session, flow, &node.id)?;
                }
                16 => {
                    let condition = eval_condition(session, flow, &node)?;
                    let label = if condition { "next_true" } else { "next_false" };
                    session.current_node =
                        next_target(&node.id, flow, label)?.ok_or_else(|| {
                            AppError::InvalidInput(format!("条件节点 {} 缺少 {label}", node.id))
                        })?;
                }
                18 => {
                    session.active_flow = "end".to_string();
                    session.current_node = runtime_rule.end_flow.entry.clone();
                }
                19 => {
                    session.deck.reverse();
                    move_to_next(session, flow, &node.id)?;
                }
                20 => {
                    execute_deal(session, &node)?;
                    move_to_next(session, flow, &node.id)?;
                }
                21 | 22 => {
                    let player_id = current_player_id(session)?;
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
                    let player_id = current_player_id(session)?;
                    session.settlement_results.insert(player_id, result);
                    session.status = "finished".to_string();
                    return Ok(());
                }
                // 值节点不会主动改变流程；如果被错误放在主链上，直接尝试走 next。
                _ => move_to_next(session, flow, &node.id)?,
            }
        }

        Err(AppError::InvalidInput(
            "规则执行超过 1000 步仍未暂停或结束，可能存在死循环".to_string(),
        ))
    }
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
        .map(|id| GamePlayer {
            id,
            properties: defaults.clone(),
        })
        .collect()
}

fn build_table(runtime_rule: &RuntimeRule) -> HashMap<String, i64> {
    runtime_rule
        .design
        .classes
        .get("table")
        .map(|class| merge_properties(&class.default_properties, &class.user_properties))
        .unwrap_or_default()
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
    flow: &RuntimeFlow,
    node: &RuntimeNode,
) -> Result<(), AppError> {
    let content = node_content(node)?;
    let component = content_string(content, "component")?;
    let rvalue = content_string(content, "rvalue")?;
    let value = eval_int(session, flow, &rvalue)?;
    assign_property(session, flow, &component, value)
}

fn execute_deal(session: &mut GameSession, node: &RuntimeNode) -> Result<(), AppError> {
    let count = node
        .count
        .as_ref()
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(0) as usize;
    let player_id = current_player_id(session)?;
    let filters = node
        .content
        .as_ref()
        .and_then(|content| content.get("prop_pair"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for _ in 0..count {
        let Some(index) = session
            .deck
            .iter()
            .position(|card| card_matches_filters(card, &filters))
        else {
            return Err(AppError::InvalidInput(
                "卡牌池中没有满足发牌条件的牌".to_string(),
            ));
        };
        let card = session.deck.remove(index);
        session
            .hands
            .entry(player_id.clone())
            .or_default()
            .push(card);
    }
    update_player_hand_count(session, &player_id);
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
    flow: &RuntimeFlow,
    node: &RuntimeNode,
) -> Result<bool, AppError> {
    let content = node_content(node)?;
    let condition = content
        .get("condition")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !condition.is_empty() {
        return Ok(match eval_value(session, flow, condition)? {
            EvalValue::Bool(value) => value,
            EvalValue::Int(value) => value != 0,
            EvalValue::Cards(cards) => !cards.is_empty(),
            EvalValue::None => false,
            EvalValue::PlayerIndex(_) | EvalValue::Card(_) => true,
        });
    }

    // 兼容当前前端样例中尚未连线的空条件：根据前后分支意图做保守推断。
    let true_target = content
        .get("next_true")
        .and_then(Value::as_str)
        .unwrap_or("");
    let false_target = content
        .get("next_false")
        .and_then(Value::as_str)
        .unwrap_or("");
    if true_target == "15" && false_target == "10" {
        return Ok(table_i64(session, "index") >= session.players.len().saturating_sub(1) as i64);
    }
    if true_target == "21" && false_target == "24" {
        return Ok(session
            .pending_action
            .as_ref()
            .map(|action| action.component_type == 21)
            .unwrap_or(false));
    }
    if true_target == "27" && false_target == "15" {
        return Ok(
            table_i64(session, "player_index") >= session.players.len().saturating_sub(1) as i64
        );
    }

    Ok(false)
}

fn eval_int(session: &GameSession, flow: &RuntimeFlow, node_id: &str) -> Result<i64, AppError> {
    match eval_value(session, flow, node_id)? {
        EvalValue::Int(value) => Ok(value),
        EvalValue::Bool(value) => Ok(i64::from(value)),
        EvalValue::PlayerIndex(index) => Ok(index as i64),
        EvalValue::Card(card) => Ok(card.properties.values().next().copied().unwrap_or_default()),
        EvalValue::Cards(cards) => Ok(cards.len() as i64),
        EvalValue::None => Ok(0),
    }
}

fn eval_value(
    session: &GameSession,
    flow: &RuntimeFlow,
    node_id: &str,
) -> Result<EvalValue, AppError> {
    let node = flow
        .nodes
        .get(node_id)
        .ok_or_else(|| AppError::InvalidInput(format!("值节点 {node_id} 不存在")))?;
    let content = node_content(node)?;

    match node.component_type {
        5 => {
            let selection = content_string(content, "selection")?;
            let index_ref = content_string(content, "index")?;
            let index = eval_int(session, flow, &index_ref)?.max(0) as usize;
            match eval_value(session, flow, &selection)? {
                EvalValue::Cards(cards) => Ok(cards
                    .get(index)
                    .cloned()
                    .map(EvalValue::Card)
                    .unwrap_or(EvalValue::None)),
                _ => Ok(session
                    .players
                    .get(index)
                    .map(|_| EvalValue::PlayerIndex(index))
                    .unwrap_or(EvalValue::None)),
            }
        }
        6 => eval_property_access(session, flow, content),
        7 => {
            let selection = content_string(content, "selection")?;
            Ok(match eval_value(session, flow, &selection)? {
                EvalValue::Cards(cards) => EvalValue::Int(cards.len() as i64),
                _ => EvalValue::Int(session.players.len() as i64),
            })
        }
        8 | 9 => Ok(EvalValue::Int(
            content.get("value").and_then(Value::as_i64).unwrap_or(0),
        )),
        10 => {
            let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
            let left = eval_int(session, flow, content_string(content, "lval")?)?;
            let right = eval_int(session, flow, content_string(content, "rval")?)?;
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
        14 => {
            let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
            let left = eval_int(session, flow, content_string(content, "lval")?)?;
            let right = eval_int(session, flow, content_string(content, "rval")?)?;
            let value = match operator {
                1 => left != right,
                2 => left > right,
                3 => left >= right,
                4 => left < right,
                5 => left <= right,
                _ => left == right,
            };
            Ok(EvalValue::Bool(value))
        }
        21 => Ok(EvalValue::Cards(
            session
                .last_successful_play
                .as_ref()
                .map(|play| play.cards.clone())
                .unwrap_or_default(),
        )),
        _ => Ok(EvalValue::None),
    }
}

fn eval_property_access(
    session: &GameSession,
    flow: &RuntimeFlow,
    content: &Value,
) -> Result<EvalValue, AppError> {
    let operator = content.get("operator").and_then(Value::as_i64).unwrap_or(0);
    let ident = content.get("ident").and_then(Value::as_str).unwrap_or("");
    let property = content
        .get("property")
        .and_then(Value::as_str)
        .unwrap_or("");

    if operator == 1 {
        if let EvalValue::Cards(cards) = eval_value(session, flow, ident)? {
            if property == "content" || property.is_empty() {
                return Ok(EvalValue::Int(cards.len() as i64));
            }
        }
    }

    if ident == "table_0" {
        if property == "玩家池" {
            return Ok(EvalValue::Int(session.players.len() as i64));
        }
        if property == "卡牌池" {
            return Ok(EvalValue::Cards(session.deck.clone()));
        }
        return Ok(EvalValue::Int(table_i64(session, property)));
    }

    if let Ok(index) = ident.parse::<usize>() {
        if let Some(player) = session.players.get(index) {
            return Ok(EvalValue::Int(
                player.properties.get(property).copied().unwrap_or_default(),
            ));
        }
    }

    Ok(EvalValue::None)
}

fn assign_property(
    session: &mut GameSession,
    flow: &RuntimeFlow,
    component_node_id: &str,
    value: i64,
) -> Result<(), AppError> {
    let node = flow.nodes.get(component_node_id).ok_or_else(|| {
        AppError::InvalidInput(format!("赋值目标节点 {component_node_id} 不存在"))
    })?;
    if node.component_type != 6 {
        return Err(AppError::InvalidInput(
            "赋值目标必须是属性访问组件".to_string(),
        ));
    }
    let content = node_content(node)?;
    let ident = content.get("ident").and_then(Value::as_str).unwrap_or("");
    let property = content
        .get("property")
        .and_then(Value::as_str)
        .unwrap_or("");

    if ident == "table_0" {
        session.table.insert(property.to_string(), value);
        return Ok(());
    }

    if let Ok(index) = ident.parse::<usize>() {
        if let Some(player) = session.players.get_mut(index) {
            player.properties.insert(property.to_string(), value);
        }
    }

    Ok(())
}

fn apply_play_cards(
    session: &mut GameSession,
    player_id: &str,
    card_ids: Vec<String>,
) -> Result<(), AppError> {
    let hand = session
        .hands
        .get_mut(player_id)
        .ok_or_else(|| AppError::InvalidInput("玩家手牌不存在".to_string()))?;
    let mut played = Vec::new();
    for card_id in card_ids {
        let Some(index) = hand.iter().position(|card| card.id == card_id) else {
            return Err(AppError::InvalidInput(format!("玩家没有手牌 {card_id}")));
        };
        played.push(hand.remove(index));
    }
    session.discard_pile.extend(played.clone());
    session.last_successful_play = Some(LastSuccessfulPlay {
        player_id: player_id.to_string(),
        cards: played,
    });
    update_player_hand_count(session, player_id);
    Ok(())
}

fn current_player_id(session: &GameSession) -> Result<String, AppError> {
    let index = table_i64(session, "player_index").max(0) as usize % session.players.len().max(1);
    session
        .players
        .get(index)
        .map(|player| player.id.clone())
        .ok_or_else(|| AppError::InvalidInput("对局中没有玩家".to_string()))
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
            compile_flow(&format!("方法「{method_name}」流程"), &method.flow)?;
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
