use crate::function_tool::FunctionCallError;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use codex_protocol::config_types::ModeKind;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::plan_tool::PlanItemArg;
use codex_protocol::plan_tool::UpdatePlanArgs;
use codex_protocol::protocol::EventMsg;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;

pub struct PlanHandler;

pub struct PlanToolOutput;

const PLAN_UPDATED_MESSAGE: &str = "Plan updated";

impl ToolOutput for PlanToolOutput {
    fn log_preview(&self) -> String {
        PLAN_UPDATED_MESSAGE.to_string()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        let mut output = FunctionCallOutputPayload::from_text(PLAN_UPDATED_MESSAGE.to_string());
        output.success = Some(true);

        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output,
        }
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        JsonValue::Object(serde_json::Map::new())
    }
}

impl ToolHandler for PlanHandler {
    type Output = PlanToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            call_id,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "update_plan handler received unsupported payload".to_string(),
                ));
            }
        };

        handle_update_plan(session.as_ref(), turn.as_ref(), arguments, call_id).await?;

        Ok(PlanToolOutput)
    }
}

/// This function doesn't do anything useful. However, it gives the model a structured way to record its plan that clients can read and render.
/// So it's the _inputs_ to this function that are useful to clients, not the outputs and neither are actually useful for the model other
/// than forcing it to come up and document a plan (TBD how that affects performance).
pub(crate) async fn handle_update_plan(
    session: &Session,
    turn_context: &TurnContext,
    arguments: String,
    _call_id: String,
) -> Result<String, FunctionCallError> {
    if turn_context.collaboration_mode.mode == ModeKind::Plan {
        return Err(FunctionCallError::RespondToModel(
            "update_plan is a TODO/checklist tool and is not allowed in Plan mode".to_string(),
        ));
    }
    let args = parse_update_plan_arguments(&arguments)?;
    session
        .send_event(turn_context, EventMsg::PlanUpdate(args))
        .await;
    Ok("Plan updated".to_string())
}

fn parse_update_plan_arguments(arguments: &str) -> Result<UpdatePlanArgs, FunctionCallError> {
    let value = serde_json::from_str::<JsonValue>(arguments).map_err(|e| {
        FunctionCallError::RespondToModel(format!("failed to parse function arguments: {e}"))
    })?;
    parse_update_plan_value(value).map_err(|e| {
        FunctionCallError::RespondToModel(format!("failed to parse function arguments: {e}"))
    })
}

fn parse_update_plan_value(value: JsonValue) -> serde_json::Result<UpdatePlanArgs> {
    let Some(object) = value.as_object() else {
        return serde_json::from_value::<UpdatePlanArgs>(value);
    };
    let Some(plan) = object.get("plan").and_then(JsonValue::as_array) else {
        return serde_json::from_value::<UpdatePlanArgs>(value);
    };

    let mut sanitized = JsonMap::new();
    if let Some(explanation) = object.get("explanation") {
        sanitized.insert("explanation".to_string(), explanation.clone());
    }
    let mut sanitized_plan = Vec::with_capacity(plan.len());
    for item in plan {
        let Some(item_object) = item.as_object() else {
            return serde_json::from_value::<UpdatePlanArgs>(JsonValue::Object(sanitized));
        };
        let mut sanitized_item = JsonMap::new();
        if let Some(step) = item_object.get("step") {
            sanitized_item.insert("step".to_string(), step.clone());
        }
        if let Some(status) = item_object.get("status") {
            sanitized_item.insert("status".to_string(), status.clone());
        }
        serde_json::from_value::<PlanItemArg>(JsonValue::Object(sanitized_item.clone()))?;
        sanitized_plan.push(JsonValue::Object(sanitized_item));
    }
    sanitized.insert("plan".to_string(), JsonValue::Array(sanitized_plan));
    serde_json::from_value::<UpdatePlanArgs>(JsonValue::Object(sanitized))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    #[test]
    fn update_plan_unknown_field_is_ignored() {
        let parsed = parse_update_plan_arguments(
            r#"{"explanation":"x","plan":[{"step":"Seed products","status":"pending","seed":true}]}"#,
        )
        .unwrap();

        assert_eq!(parsed.plan.len(), 1);
        assert_eq!(parsed.plan[0].step, "Seed products");
        assert!(matches!(
            parsed.plan[0].status,
            codex_protocol::plan_tool::StepStatus::Pending
        ));
        assert_eq!(parsed.explanation, Some("x".to_string()));
    }

    #[test]
    fn update_plan_missing_required_fields_is_controlled_error() {
        let error =
            parse_update_plan_arguments(r#"{"plan":[{"step":"Seed products"}]}"#).unwrap_err();

        assert!(matches!(error, FunctionCallError::RespondToModel(_)));
    }
}
