use serde_json::Value;

/// Validate the JSON Schema vocabulary used by this server's self-contained schemas.
/// No remote references, arbitrary schemas, or network fetching are supported.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    if let Some(choices) = schema["oneOf"].as_array() {
        if choices
            .iter()
            .filter(|s| validate(s, value).is_ok())
            .count()
            != 1
        {
            return Err("Value does not match exactly one operation schema".into());
        }
    }
    if schema
        .get("const")
        .is_some_and(|expected| expected != value)
    {
        return Err("Value differs from the required constant".into());
    }
    if let Some(choices) = schema["enum"].as_array() {
        if !choices.contains(value) {
            return Err("Value is outside the allowed enum".into());
        }
    }
    let matches = |t: &str| match t {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        _ => false,
    };
    if let Some(t) = schema["type"].as_str() {
        if !matches(t) {
            return Err(format!("Expected {t}"));
        }
    }
    if let Some(types) = schema["type"].as_array() {
        if !types.iter().any(|t| t.as_str().is_some_and(matches)) {
            return Err("Unexpected value type".into());
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for key in required {
                if !object.contains_key(key.as_str().unwrap_or("")) {
                    return Err(format!("Missing property: {key}"));
                }
            }
        }
        for (key, v) in object {
            if let Some(s) = schema["properties"].get(key) {
                validate(s, v).map_err(|e| format!("{key}: {e}"))?;
            } else if schema["additionalProperties"] == false {
                return Err(format!("Unexpected property: {key}"));
            } else if schema["additionalProperties"].is_object() {
                validate(&schema["additionalProperties"], v)?;
            }
        }
    }
    if let Some(array) = value.as_array() {
        if schema["minItems"]
            .as_u64()
            .is_some_and(|n| array.len() < (n as usize))
            || schema["maxItems"]
                .as_u64()
                .is_some_and(|n| array.len() > (n as usize))
        {
            return Err("Array length out of range".into());
        }
        if let Some(items) = schema.get("items") {
            for v in array {
                validate(items, v)?;
            }
        }
    }
    if let Some(s) = value.as_str() {
        let len = s.chars().count();
        if schema["minLength"]
            .as_u64()
            .is_some_and(|n| len < (n as usize))
            || schema["maxLength"]
                .as_u64()
                .is_some_and(|n| len > (n as usize))
        {
            return Err("String length out of range".into());
        }
    }
    if let Some(n) = value.as_f64() {
        if schema["minimum"].as_f64().is_some_and(|min| n < min)
            || schema["maximum"].as_f64().is_some_and(|max| n > max)
        {
            return Err("Number out of range".into());
        }
    }
    Ok(())
}
