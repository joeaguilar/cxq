use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub input: Value,
}

pub fn timestamp(value: &Value) -> Option<&str> {
    value.get("timestamp")?.as_str()
}

pub fn session_meta(value: &Value) -> Option<(&str, &str, &str)> {
    if value.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let payload = value.get("payload")?;
    Some((
        payload.get("id")?.as_str()?,
        payload.get("cwd").and_then(Value::as_str).unwrap_or(""),
        payload
            .get("cli_version")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ))
}

pub fn event_message<'a>(value: &'a Value, wanted: &str) -> Option<&'a str> {
    if value.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != wanted {
        return None;
    }
    payload.get("message")?.as_str()
}

pub fn tool_call(value: &Value) -> Option<ToolCall> {
    if value.get("type")?.as_str()? != "response_item" {
        return None;
    }
    let payload = value.get("payload")?;
    let item_type = payload.get("type")?.as_str()?;
    if !matches!(item_type, "function_call" | "custom_tool_call") {
        return None;
    }

    let name = payload.get("name")?.as_str()?.to_owned();
    let raw = payload
        .get("arguments")
        .or_else(|| payload.get("input"))
        .cloned()
        .unwrap_or(Value::Null);
    let input = raw
        .as_str()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(raw);
    Some(ToolCall { name, input })
}

pub fn tool_output(value: &Value) -> Option<(&str, String)> {
    if value.get("type")?.as_str()? != "response_item" {
        return None;
    }
    let payload = value.get("payload")?;
    if !matches!(
        payload.get("type")?.as_str()?,
        "function_call_output" | "custom_tool_call_output"
    ) {
        return None;
    }
    let call_id = payload.get("call_id").and_then(Value::as_str).unwrap_or("");
    let output = match payload.get("output") {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    Some((call_id, output))
}

pub fn assistant_message(value: &Value) -> Option<&str> {
    event_message(value, "agent_message")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_encoded_function_arguments() {
        let value: Value = serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "function_call",
                "name": "exec_command",
                "call_id": "call-1",
                "arguments": "{\"cmd\":\"cargo test\"}"
            }
        });
        let call = tool_call(&value).expect("tool call");
        assert_eq!(call.name, "exec_command");
        assert_eq!(call.input["cmd"], "cargo test");
    }

    #[test]
    fn ignores_reasoning_items() {
        let value: Value = serde_json::json!({
            "type": "response_item",
            "payload": {"type": "reasoning", "encrypted_content": "secret"}
        });
        assert!(tool_call(&value).is_none());
        assert!(assistant_message(&value).is_none());
    }
}
