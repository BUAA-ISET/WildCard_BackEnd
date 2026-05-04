use crate::domain::rule::{
    Expression, FlowGraph, NativeMethod, RuleDefinition, RuleEngine, RuleError,
    RuleExecutionContext, RuleObject, RuleValue,
};
use crate::error::AppError;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct ExecuteRuleRequest {
    pub rule: RuleDefinition,
    pub flow: FlowGraph,
    pub start_node: String,
    #[serde(default)]
    pub variables: BTreeMap<String, RuleValue>,
    #[serde(default)]
    pub objects: BTreeMap<String, BTreeMap<String, RuleValue>>,
    #[serde(default)]
    pub probe_expression: Option<Expression>,
}

#[derive(Debug, Serialize)]
pub struct ExecuteRuleResponse {
    pub rule_name: String,
    pub returned: Option<RuleValue>,
    pub events: Vec<crate::domain::rule::RuleRuntimeEvent>,
    pub probe_result: Option<RuleValue>,
}

fn bump_native_method(
    _context: &mut RuleExecutionContext,
    args: Vec<RuleValue>,
) -> Result<RuleValue, RuleError> {
    let value = args
        .first()
        .and_then(RuleValue::as_integer)
        .ok_or_else(|| {
            RuleError::InvalidValue("native method missing integer argument".to_string())
        })?;
    Ok(RuleValue::Integer(value + 1))
}

fn normalize_object(properties: BTreeMap<String, RuleValue>) -> RuleObject {
    let mut object = RuleObject::new(properties);
    let _ = object.get("online");
    if let Some(online) = object.get_mut("online") {
        *online = RuleValue::Boolean(true);
    } else {
        object
            .properties
            .insert("online".to_string(), RuleValue::Boolean(true));
    }
    let object_value = object.into_value();
    RuleObject::from_value(object_value).expect("object round-trip must succeed")
}

#[tracing::instrument(skip(payload))]
pub async fn execute_rule_handler(
    Json(payload): Json<ExecuteRuleRequest>,
) -> Result<Json<ExecuteRuleResponse>, AppError> {
    let engine = RuleEngine::new(payload.rule.clone());
    let rule_definition = engine.definition();

    let mut context = RuleExecutionContext::default();
    context.insert_variable("__rule_name", RuleValue::Text(rule_definition.name.clone()));
    context.insert_variable(
        "__player_count",
        RuleValue::Integer(rule_definition.player_count as i64),
    );
    context.register_native_method("player.bump", bump_native_method as NativeMethod);
    let _ = context.call_native_method("player.bump", vec![RuleValue::Integer(0)]);

    for (name, object) in payload.objects {
        context.insert_object(name, normalize_object(object));
    }
    for (name, value) in payload.variables {
        context.insert_variable(name, value);
    }

    let probe_result = match payload.probe_expression {
        Some(expression) => Some(engine.evaluate_expression(&expression, &mut context)?),
        None => None,
    };

    let execution = engine.execute_flow(&payload.flow, &payload.start_node, &mut context)?;

    Ok(Json(ExecuteRuleResponse {
        rule_name: rule_definition.name.clone(),
        returned: execution.returned,
        events: execution.events,
        probe_result,
    }))
}
