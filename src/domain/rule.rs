use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

pub type NodeId = String;
pub type FlowGraph = BTreeMap<NodeId, FlowNode>;
pub type NativeMethod =
    fn(&mut RuleExecutionContext, Vec<RuleValue>) -> Result<RuleValue, RuleError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleValue {
    Integer(i64),
    Enum(i64),
    Boolean(bool),
    Text(String),
    List(Vec<RuleValue>),
    Object(BTreeMap<String, RuleValue>),
    Null,
}

impl RuleValue {
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            RuleValue::Integer(value) | RuleValue::Enum(value) => Some(*value),
            RuleValue::Boolean(value) => Some(i64::from(*value)),
            RuleValue::Text(_) | RuleValue::List(_) | RuleValue::Object(_) | RuleValue::Null => {
                None
            }
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            RuleValue::Boolean(value) => Some(*value),
            RuleValue::Integer(value) | RuleValue::Enum(value) => Some(*value != 0),
            RuleValue::Text(_) | RuleValue::Null => None,
            RuleValue::List(_) | RuleValue::Object(_) => None,
        }
    }

    pub fn compare_scalar(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (RuleValue::Integer(left), RuleValue::Integer(right))
            | (RuleValue::Integer(left), RuleValue::Enum(right))
            | (RuleValue::Enum(left), RuleValue::Integer(right))
            | (RuleValue::Enum(left), RuleValue::Enum(right)) => Some(left.cmp(right)),
            (RuleValue::Boolean(left), RuleValue::Boolean(right)) => Some(left.cmp(right)),
            (RuleValue::Text(left), RuleValue::Text(right)) => Some(left.cmp(right)),
            (RuleValue::Null, RuleValue::Null) => Some(Ordering::Equal),
            (RuleValue::Null, _) => Some(Ordering::Less),
            (_, RuleValue::Null) => Some(Ordering::Greater),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[RuleValue]> {
        match self {
            RuleValue::List(values) => Some(values.as_slice()),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, RuleValue>> {
        match self {
            RuleValue::Object(values) => Some(values),
            _ => None,
        }
    }

    pub fn into_object(self) -> Option<BTreeMap<String, RuleValue>> {
        match self {
            RuleValue::Object(values) => Some(values),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValueType {
    Integer,
    Collection(Box<ValueType>),
    Enum {
        class_name: String,
        property_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnumOption {
    pub display: String,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropertyDefinition {
    #[serde(rename = "type")]
    pub value_type: ValueType,
    pub default: RuleValue,
    #[serde(default)]
    pub config: Vec<EnumOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MethodParameter {
    #[serde(rename = "type")]
    pub value_type: ValueType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MethodDefinition {
    #[serde(default)]
    pub parameters: BTreeMap<String, MethodParameter>,
    pub returns: Option<ValueType>,
    #[serde(default)]
    pub flow: FlowGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ClassDefinition {
    #[serde(default)]
    pub default_properties: BTreeMap<String, PropertyDefinition>,
    #[serde(default)]
    pub user_properties: BTreeMap<String, PropertyDefinition>,
    #[serde(default)]
    pub methods: BTreeMap<String, MethodDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CardSetDefinition {
    pub name: String,
    #[serde(default)]
    pub properties: BTreeMap<String, PropertyDefinition>,
    #[serde(default)]
    pub build_flow: FlowGraph,
    #[serde(default)]
    pub compare_flow: FlowGraph,
    #[serde(default)]
    pub successors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuleDefinition {
    pub name: String,
    pub player_count: usize,
    #[serde(default)]
    pub classes: BTreeMap<String, ClassDefinition>,
    #[serde(default)]
    pub cardsets: BTreeMap<String, CardSetDefinition>,
    #[serde(default)]
    pub match_flow: FlowGraph,
    #[serde(default)]
    pub end_flow: FlowGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleObject {
    #[serde(default)]
    pub properties: BTreeMap<String, RuleValue>,
}

impl RuleObject {
    pub fn new(properties: BTreeMap<String, RuleValue>) -> Self {
        Self { properties }
    }

    pub fn get(&self, property_name: &str) -> Option<&RuleValue> {
        self.properties.get(property_name)
    }

    pub fn get_mut(&mut self, property_name: &str) -> Option<&mut RuleValue> {
        self.properties.get_mut(property_name)
    }

    pub fn into_value(self) -> RuleValue {
        RuleValue::Object(self.properties)
    }

    pub fn from_value(value: RuleValue) -> Option<Self> {
        value.into_object().map(Self::new)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleRuntimeEvent {
    pub name: String,
    #[serde(default)]
    pub payload: BTreeMap<String, RuleValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuleExecutionContext {
    #[serde(default)]
    pub variables: BTreeMap<String, RuleValue>,
    #[serde(default)]
    pub objects: BTreeMap<String, RuleObject>,
    #[serde(default)]
    pub events: Vec<RuleRuntimeEvent>,
    #[serde(skip, default)]
    pub native_methods: BTreeMap<String, NativeMethod>,
}

impl RuleExecutionContext {
    pub fn insert_variable(&mut self, name: impl Into<String>, value: RuleValue) {
        self.variables.insert(name.into(), value);
    }

    pub fn insert_object(&mut self, name: impl Into<String>, object: RuleObject) {
        self.objects.insert(name.into(), object);
    }

    pub fn register_native_method(&mut self, name: impl Into<String>, method: NativeMethod) {
        self.native_methods.insert(name.into(), method);
    }

    pub fn call_native_method(
        &mut self,
        name: &str,
        args: Vec<RuleValue>,
    ) -> Result<RuleValue, RuleError> {
        let method = self
            .native_methods
            .get(name)
            .copied()
            .ok_or_else(|| RuleError::Unsupported(format!("method not registered: {name}")))?;
        method(self, args)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "data")]
pub enum Expression {
    Constant(RuleValue),
    Variable(String),
    Property {
        object: String,
        property: String,
    },
    CollectionAccess {
        collection: Box<Expression>,
        index: Box<Expression>,
    },
    CollectionSize(Box<Expression>),
    Arithmetic {
        op: ArithmeticOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Compare {
        op: ComparisonOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Logical {
        op: LogicalOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    MethodCall {
        object: String,
        method: String,
        #[serde(default)]
        args: Vec<Expression>,
    },
    Not(Box<Expression>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComparisonOperator {
    Greater,
    Less,
    Equal,
    GreaterOrEqual,
    LessOrEqual,
    NotEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LogicalOperator {
    And,
    Or,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FlowStartKind {
    #[default]
    Generic,
    Game {
        table: String,
    },
    Method {
        self_object: String,
        parameters: Vec<String>,
    },
    Match {
        cards: String,
    },
    Compare {
        left: String,
        right: String,
    },
    Settlement {
        player: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SelectionMode {
    All,
    Random { count: usize },
    Filter { predicate: Expression },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SortKey {
    pub property: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompareWinner {
    A,
    B,
    Tie,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "content")]
pub enum FlowNodeContent {
    Start {
        #[serde(default)]
        kind: FlowStartKind,
        next: Option<NodeId>,
    },
    Nop {
        next: Option<NodeId>,
    },
    Assignment {
        target: String,
        value: Expression,
        next: Option<NodeId>,
    },
    Branch {
        condition: Expression,
        on_true: Option<NodeId>,
        on_false: Option<NodeId>,
    },
    SortCollection {
        collection: String,
        #[serde(default)]
        keys: Vec<SortKey>,
        next: Option<NodeId>,
    },
    SelectCollection {
        source_collection: String,
        target_collection: String,
        mode: SelectionMode,
        #[serde(default)]
        current_item: Option<String>,
        next: Option<NodeId>,
    },
    Emit {
        name: String,
        #[serde(default)]
        payload: BTreeMap<String, RuleValue>,
        next: Option<NodeId>,
    },
    Call {
        object: String,
        method: String,
        #[serde(default)]
        args: Vec<Expression>,
        has_return: bool,
        next: Option<NodeId>,
    },
    MatchCards {
        cards: Expression,
        matched: Expression,
        #[serde(default)]
        attributes: BTreeMap<String, Expression>,
        next: Option<NodeId>,
    },
    CompareReturn {
        winner: CompareWinner,
    },
    Return {
        value: Option<Expression>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowNode {
    pub content: FlowNodeContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleExecutionResult {
    pub returned: Option<RuleValue>,
    #[serde(default)]
    pub events: Vec<RuleRuntimeEvent>,
}

#[derive(Debug, Clone)]
pub struct RuleEngine {
    definition: RuleDefinition,
}

#[derive(Debug)]
pub enum RuleError {
    MissingNode(String),
    MissingValue(String),
    InvalidValue(String),
    Unsupported(String),
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleError::MissingNode(node) => write!(f, "missing rule node: {node}"),
            RuleError::MissingValue(name) => write!(f, "missing rule value: {name}"),
            RuleError::InvalidValue(message) => write!(f, "invalid rule value: {message}"),
            RuleError::Unsupported(message) => write!(f, "unsupported rule feature: {message}"),
        }
    }
}

impl std::error::Error for RuleError {}

impl RuleEngine {
    pub fn new(definition: RuleDefinition) -> Self {
        Self { definition }
    }

    pub fn definition(&self) -> &RuleDefinition {
        &self.definition
    }

    pub fn execute_flow(
        &self,
        flow: &FlowGraph,
        start_node: &str,
        context: &mut RuleExecutionContext,
    ) -> Result<RuleExecutionResult, RuleError> {
        let definition = self.definition();
        context.events.push(RuleRuntimeEvent {
            name: "flow_start".to_string(),
            payload: BTreeMap::from([
                (
                    "rule_name".to_string(),
                    RuleValue::Text(definition.name.clone()),
                ),
                (
                    "player_count".to_string(),
                    RuleValue::Integer(definition.player_count as i64),
                ),
            ]),
        });
        let mut cursor = start_node.to_string();

        loop {
            let node = flow
                .get(&cursor)
                .ok_or_else(|| RuleError::MissingNode(cursor.clone()))?;

            match &node.content {
                FlowNodeContent::Start { next, .. } | FlowNodeContent::Nop { next } => {
                    cursor = self.follow(next)?;
                }
                FlowNodeContent::Assignment {
                    target,
                    value,
                    next,
                } => {
                    let evaluated = self.evaluate_expression(value, context)?;
                    if let Some((object_name, property_name)) = target.split_once('.') {
                        let object = context
                            .objects
                            .get_mut(object_name)
                            .ok_or_else(|| RuleError::MissingValue(object_name.to_string()))?;
                        let property = object.get_mut(property_name).ok_or_else(|| {
                            RuleError::MissingValue(format!("{object_name}.{property_name}"))
                        })?;
                        *property = evaluated;
                    } else {
                        context.insert_variable(target.clone(), evaluated);
                    }
                    cursor = self.follow(next)?;
                }
                FlowNodeContent::Branch {
                    condition,
                    on_true,
                    on_false,
                } => {
                    let condition_value = self.evaluate_expression(condition, context)?;
                    cursor = if condition_value.as_bool().ok_or_else(|| {
                        RuleError::InvalidValue("branch condition is not boolean".to_string())
                    })? {
                        self.follow(on_true)?
                    } else {
                        self.follow(on_false)?
                    };
                }
                FlowNodeContent::SortCollection {
                    collection,
                    keys,
                    next,
                } => {
                    self.execute_sort_collection(collection, keys, context)?;
                    cursor = self.follow(next)?;
                }
                FlowNodeContent::SelectCollection {
                    source_collection,
                    target_collection,
                    mode,
                    current_item,
                    next,
                } => {
                    self.execute_selection(
                        source_collection,
                        target_collection,
                        mode,
                        current_item,
                        context,
                    )?;
                    cursor = self.follow(next)?;
                }
                FlowNodeContent::Emit {
                    name,
                    payload,
                    next,
                } => {
                    context.events.push(RuleRuntimeEvent {
                        name: name.clone(),
                        payload: payload.clone(),
                    });
                    cursor = self.follow(next)?;
                }
                FlowNodeContent::Call {
                    object,
                    method,
                    args,
                    has_return,
                    next,
                } => {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for argument in args {
                        evaluated_args.push(self.evaluate_expression(argument, context)?);
                    }
                    let _ = context
                        .call_native_method(&format!("{object}.{method}"), evaluated_args)?;
                    let mut payload = BTreeMap::new();
                    payload.insert("object".to_string(), RuleValue::Text(object.clone()));
                    payload.insert("method".to_string(), RuleValue::Text(method.clone()));
                    payload.insert(
                        "argument_count".to_string(),
                        RuleValue::Integer(args.len() as i64),
                    );
                    payload.insert("has_return".to_string(), RuleValue::Boolean(*has_return));
                    context.events.push(RuleRuntimeEvent {
                        name: "call".to_string(),
                        payload,
                    });
                    cursor = self.follow(next)?;
                }
                FlowNodeContent::MatchCards {
                    cards,
                    matched,
                    attributes,
                    next,
                } => {
                    let cards_value = self.evaluate_expression(cards, context)?;
                    let matched_value = self.evaluate_expression(matched, context)?;
                    let matched_bool = matched_value.as_bool().ok_or_else(|| {
                        RuleError::InvalidValue("match result is not boolean".to_string())
                    })?;
                    let mut payload = BTreeMap::new();
                    payload.insert("matched".to_string(), RuleValue::Boolean(matched_bool));
                    payload.insert("cards".to_string(), cards_value.clone());
                    for (name, expression) in attributes {
                        let value = self.evaluate_expression(expression, context)?;
                        context.insert_variable(name.clone(), value.clone());
                        payload.insert(name.clone(), value);
                    }
                    context.events.push(RuleRuntimeEvent {
                        name: "match_cards".to_string(),
                        payload,
                    });
                    cursor = self.follow(next)?;
                }
                FlowNodeContent::CompareReturn { winner } => {
                    let result = match winner {
                        CompareWinner::A => RuleValue::Text("A".to_string()),
                        CompareWinner::B => RuleValue::Text("B".to_string()),
                        CompareWinner::Tie => RuleValue::Text("Tie".to_string()),
                    };
                    context.events.push(RuleRuntimeEvent {
                        name: "compare_return".to_string(),
                        payload: BTreeMap::from([("winner".to_string(), result.clone())]),
                    });
                    return Ok(RuleExecutionResult {
                        returned: Some(result),
                        events: context.events.clone(),
                    });
                }
                FlowNodeContent::Return { value } => {
                    let returned = match value {
                        Some(expression) => Some(self.evaluate_expression(expression, context)?),
                        None => None,
                    };
                    return Ok(RuleExecutionResult {
                        returned,
                        events: context.events.clone(),
                    });
                }
            }
        }
    }

    fn follow(&self, next: &Option<NodeId>) -> Result<String, RuleError> {
        next.clone()
            .ok_or_else(|| RuleError::Unsupported("flow ended without a next node".to_string()))
    }

    pub fn evaluate_expression(
        &self,
        expression: &Expression,
        context: &mut RuleExecutionContext,
    ) -> Result<RuleValue, RuleError> {
        match expression {
            Expression::Constant(value) => Ok(value.clone()),
            Expression::Variable(name) => context
                .variables
                .get(name)
                .cloned()
                .ok_or_else(|| RuleError::MissingValue(name.clone())),
            Expression::Property { object, property } => context
                .objects
                .get(object)
                .and_then(|item| item.get(property))
                .cloned()
                .ok_or_else(|| RuleError::MissingValue(format!("{object}.{property}"))),
            Expression::CollectionAccess { collection, index } => {
                let collection_value = self.evaluate_expression(collection, context)?;
                let index = self
                    .evaluate_expression(index, context)?
                    .as_integer()
                    .ok_or_else(|| {
                        RuleError::InvalidValue("collection index is not an integer".to_string())
                    })?;
                let collection = collection_value.as_list().ok_or_else(|| {
                    RuleError::InvalidValue("collection access target is not a list".to_string())
                })?;
                let index = usize::try_from(index).map_err(|_| {
                    RuleError::InvalidValue("collection index cannot be negative".to_string())
                })?;
                collection
                    .get(index)
                    .cloned()
                    .ok_or_else(|| RuleError::MissingValue(format!("collection index {index}")))
            }
            Expression::CollectionSize(collection) => {
                let collection = self.evaluate_expression(collection, context)?;
                let size = collection
                    .as_list()
                    .ok_or_else(|| {
                        RuleError::InvalidValue("size target is not a list".to_string())
                    })?
                    .len() as i64;
                Ok(RuleValue::Integer(size))
            }
            Expression::Arithmetic { op, left, right } => {
                let left = self
                    .evaluate_expression(left, context)?
                    .as_integer()
                    .ok_or_else(|| {
                        RuleError::InvalidValue("left operand is not an integer".to_string())
                    })?;
                let right = self
                    .evaluate_expression(right, context)?
                    .as_integer()
                    .ok_or_else(|| {
                        RuleError::InvalidValue("right operand is not an integer".to_string())
                    })?;

                let value = match op {
                    ArithmeticOperator::Add => left + right,
                    ArithmeticOperator::Subtract => left - right,
                    ArithmeticOperator::Multiply => left * right,
                    ArithmeticOperator::Divide => left / right,
                    ArithmeticOperator::Modulo => left % right,
                };

                Ok(RuleValue::Integer(value))
            }
            Expression::Compare { op, left, right } => {
                let left = self.evaluate_expression(left, context)?;
                let right = self.evaluate_expression(right, context)?;
                let value = match op {
                    ComparisonOperator::Greater => left.as_integer() > right.as_integer(),
                    ComparisonOperator::Less => left.as_integer() < right.as_integer(),
                    ComparisonOperator::Equal => left == right,
                    ComparisonOperator::GreaterOrEqual => left.as_integer() >= right.as_integer(),
                    ComparisonOperator::LessOrEqual => left.as_integer() <= right.as_integer(),
                    ComparisonOperator::NotEqual => left != right,
                };
                Ok(RuleValue::Boolean(value))
            }
            Expression::Logical { op, left, right } => {
                let left = self
                    .evaluate_expression(left, context)?
                    .as_bool()
                    .ok_or_else(|| {
                        RuleError::InvalidValue("left operand is not boolean".to_string())
                    })?;
                let right = self
                    .evaluate_expression(right, context)?
                    .as_bool()
                    .ok_or_else(|| {
                        RuleError::InvalidValue("right operand is not boolean".to_string())
                    })?;

                let value = match op {
                    LogicalOperator::And => left && right,
                    LogicalOperator::Or => left || right,
                };
                Ok(RuleValue::Boolean(value))
            }
            Expression::MethodCall {
                object,
                method,
                args,
            } => {
                let key = format!("{object}.{method}");
                let mut evaluated_args = Vec::with_capacity(args.len());
                for argument in args {
                    evaluated_args.push(self.evaluate_expression(argument, context)?);
                }
                let result = context.call_native_method(&key, evaluated_args)?;
                context.events.push(RuleRuntimeEvent {
                    name: "method_call".to_string(),
                    payload: BTreeMap::from([
                        ("object".to_string(), RuleValue::Text(object.clone())),
                        ("method".to_string(), RuleValue::Text(method.clone())),
                    ]),
                });
                Ok(result)
            }
            Expression::Not(inner) => {
                let value = self
                    .evaluate_expression(inner, context)?
                    .as_bool()
                    .ok_or_else(|| RuleError::InvalidValue("operand is not boolean".to_string()))?;
                Ok(RuleValue::Boolean(!value))
            }
        }
    }

    fn execute_sort_collection(
        &self,
        collection_name: &str,
        keys: &[SortKey],
        context: &mut RuleExecutionContext,
    ) -> Result<(), RuleError> {
        let collection = context
            .variables
            .get_mut(collection_name)
            .ok_or_else(|| RuleError::MissingValue(collection_name.to_string()))?;
        let values = match collection {
            RuleValue::List(values) => values,
            _ => {
                return Err(RuleError::InvalidValue(format!(
                    "{collection_name} is not a list"
                )));
            }
        };

        values.sort_by(|left, right| self.compare_sorted_objects(left, right, keys));
        Ok(())
    }

    fn compare_sorted_objects(
        &self,
        left: &RuleValue,
        right: &RuleValue,
        keys: &[SortKey],
    ) -> Ordering {
        for key in keys {
            let ordering = match (left, right) {
                (RuleValue::Object(left_map), RuleValue::Object(right_map)) => {
                    let left_value = left_map.get(&key.property).unwrap_or(&RuleValue::Null);
                    let right_value = right_map.get(&key.property).unwrap_or(&RuleValue::Null);
                    left_value
                        .compare_scalar(right_value)
                        .unwrap_or(Ordering::Equal)
                }
                _ => left.compare_scalar(right).unwrap_or(Ordering::Equal),
            };

            let ordering = match key.direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            };

            if ordering != Ordering::Equal {
                return ordering;
            }
        }

        Ordering::Equal
    }

    fn execute_selection(
        &self,
        source_collection: &str,
        target_collection: &str,
        mode: &SelectionMode,
        current_item: &Option<String>,
        context: &mut RuleExecutionContext,
    ) -> Result<(), RuleError> {
        let source = context
            .variables
            .get(source_collection)
            .ok_or_else(|| RuleError::MissingValue(source_collection.to_string()))?;
        let items = source
            .as_list()
            .ok_or_else(|| RuleError::InvalidValue(format!("{source_collection} is not a list")))?;
        let mut selected: Vec<RuleValue> = Vec::new();

        match mode {
            SelectionMode::All => {
                selected.extend(items.iter().cloned());
            }
            SelectionMode::Random { count } => {
                let mut pool = items.to_vec();
                let mut rng = rand::rng();
                let limit = (*count).min(pool.len());
                while selected.len() < limit {
                    let index = rng.random_range(0..pool.len());
                    selected.push(pool.swap_remove(index));
                }
            }
            SelectionMode::Filter { predicate } => {
                for item in items {
                    let mut scoped = context.clone();
                    if let Some(name) = current_item {
                        if let Some(object) = RuleObject::from_value(item.clone()) {
                            scoped.insert_object(name.clone(), object);
                        } else {
                            scoped.insert_variable(name.clone(), item.clone());
                        }
                    }

                    let keep = self
                        .evaluate_expression(predicate, &mut scoped)?
                        .as_bool()
                        .ok_or_else(|| {
                            RuleError::InvalidValue(
                                "selection predicate is not boolean".to_string(),
                            )
                        })?;
                    if keep {
                        if let Some(object) = RuleObject::from_value(item.clone()) {
                            selected.push(object.into_value());
                        } else {
                            selected.push(item.clone());
                        }
                    }
                }
            }
        }

        context
            .variables
            .insert(target_collection.to_string(), RuleValue::List(selected));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bump_score(
        _context: &mut RuleExecutionContext,
        args: Vec<RuleValue>,
    ) -> Result<RuleValue, RuleError> {
        let value = args
            .first()
            .and_then(RuleValue::as_integer)
            .ok_or_else(|| RuleError::InvalidValue("missing score argument".to_string()))?;
        Ok(RuleValue::Integer(value + 1))
    }

    #[test]
    fn sorts_and_accesses_collections() {
        let mut context = RuleExecutionContext::default();
        context.insert_variable(
            "cards",
            RuleValue::List(vec![
                RuleValue::Object(BTreeMap::from([
                    ("rank".to_string(), RuleValue::Integer(3)),
                    ("name".to_string(), RuleValue::Text("c".to_string())),
                ])),
                RuleValue::Object(BTreeMap::from([
                    ("rank".to_string(), RuleValue::Integer(1)),
                    ("name".to_string(), RuleValue::Text("a".to_string())),
                ])),
                RuleValue::Object(BTreeMap::from([
                    ("rank".to_string(), RuleValue::Integer(2)),
                    ("name".to_string(), RuleValue::Text("b".to_string())),
                ])),
            ]),
        );

        let flow = FlowGraph::from([
            (
                "1".to_string(),
                FlowNode {
                    content: FlowNodeContent::Start {
                        kind: FlowStartKind::Generic,
                        next: Some("2".to_string()),
                    },
                },
            ),
            (
                "2".to_string(),
                FlowNode {
                    content: FlowNodeContent::SortCollection {
                        collection: "cards".to_string(),
                        keys: vec![SortKey {
                            property: "rank".to_string(),
                            direction: SortDirection::Ascending,
                        }],
                        next: Some("3".to_string()),
                    },
                },
            ),
            (
                "3".to_string(),
                FlowNode {
                    content: FlowNodeContent::Return {
                        value: Some(Expression::CollectionAccess {
                            collection: Box::new(Expression::Variable("cards".to_string())),
                            index: Box::new(Expression::Constant(RuleValue::Integer(0))),
                        }),
                    },
                },
            ),
        ]);

        let engine = RuleEngine::new(RuleDefinition::default());
        let result = engine.execute_flow(&flow, "1", &mut context).unwrap();
        let returned = result.returned.unwrap();
        let object = RuleObject::from_value(returned).unwrap();
        assert_eq!(object.get("rank"), Some(&RuleValue::Integer(1)));
    }

    #[test]
    fn calls_registered_native_method() {
        let mut context = RuleExecutionContext::default();
        context.register_native_method("player.bump", bump_score);

        let value = RuleEngine::new(RuleDefinition::default())
            .evaluate_expression(
                &Expression::MethodCall {
                    object: "player".to_string(),
                    method: "bump".to_string(),
                    args: vec![Expression::Constant(RuleValue::Integer(4))],
                },
                &mut context,
            )
            .unwrap();

        assert_eq!(value, RuleValue::Integer(5));
    }
}
