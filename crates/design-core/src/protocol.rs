use serde_json::{json, Value};
// Schemas are shared by MCP discovery and the GUI command service.
pub fn tools() -> Vec<Value> {
    let revision = json!({"type":"integer","minimum":0});
    let identifier = json!({"type":"string","minLength":1,"maxLength":64});
    let kind = json!({"enum":["input","output","add","sub","and","or","xor","not","mux"]});
    let node = json!({"type":"object","required":["id","name","kind","width","x","y"],"additionalProperties":false,"properties":{"id":identifier,"name":identifier,"kind":kind,"width":{"type":"integer","minimum":1,"maximum":64},"x":{"type":"number","minimum":-100000,"maximum":100000},"y":{"type":"number","minimum":-100000,"maximum":100000}}});
    let edge = json!({"type":"object","required":["id","from","to","port"],"additionalProperties":false,"properties":{"id":identifier,"from":identifier,"to":identifier,"port":{"enum":["a","b","d0","d1","d2","sel"]}}});
    let op = json!({"oneOf":[
        {"type":"object","required":["op","node"],"additionalProperties":false,"properties":{"op":{"enum":["add_node"]},"node":node}},
        {"type":"object","required":["op","id"],"additionalProperties":false,"properties":{"op":{"enum":["remove_node","disconnect"]},"id":identifier}},
        {"type":"object","required":["op","id"],"additionalProperties":false,"properties":{"op":{"enum":["update_node"]},"id":identifier,"name":{"type":["string","null"],"minLength":1,"maxLength":64},"kind":{"enum":["input","output","add","sub","and","or","xor","not","mux",null]},"width":{"type":["integer","null"],"minimum":1,"maximum":64},"x":{"type":["number","null"],"minimum":-100000,"maximum":100000},"y":{"type":["number","null"],"minimum":-100000,"maximum":100000}}},
        {"type":"object","required":["op","edge"],"additionalProperties":false,"properties":{"op":{"enum":["connect"]},"edge":edge}}
    ]});
    let vectors = json!({"type":"array","minItems":1,"maxItems":4096,"items":{"type":"object","required":["inputs","expected"],"additionalProperties":false,"properties":{"inputs":{"type":"object","additionalProperties":{"type":"string"}},"expected":{"type":"object","additionalProperties":{"type":"string"}}}}});
    let patch = json!({"expected_revision":revision,"label":{"type":"string","maxLength":160},"operations":{"type":"array","items":op,"minItems":1,"maxItems":512}});
    let empty = json!({});
    let defs = vec![
        (
            "project.inspect",
            "Read shared workspace, revision, diagnostics and results",
            empty.clone(),
            vec![],
        ),
        (
            "design.get_ir",
            "Read the canonical Architecture IR",
            empty.clone(),
            vec![],
        ),
        (
            "design.propose_patch",
            "Validate and preview a patch without changing the design",
            patch.clone(),
            vec!["expected_revision", "label", "operations"],
        ),
        (
            "design.apply_patch",
            "Apply a revision-checked atomic patch",
            patch,
            vec!["expected_revision", "label", "operations"],
        ),
        (
            "design.undo",
            "Undo the previous edit",
            json!({"expected_revision":revision}),
            vec!["expected_revision"],
        ),
        (
            "design.redo",
            "Redo an undone edit",
            json!({"expected_revision":revision}),
            vec!["expected_revision"],
        ),
        (
            "layout.auto_arrange",
            "Lay out the DAG in topological columns",
            json!({"expected_revision":revision}),
            vec!["expected_revision"],
        ),
        (
            "view.focus_module",
            "Select a node in every connected view",
            json!({"id":{"type":"string"}}),
            vec!["id"],
        ),
        (
            "rtl.generate",
            "Generate synthesizable Verilog from the validated IR",
            empty.clone(),
            vec![],
        ),
        (
            "simulation.evaluate",
            "Evaluate input values; use decimal or hex strings",
            json!({"inputs":{"type":"object","additionalProperties":{"type":"string"}}}),
            vec!["inputs"],
        ),
        (
            "verify.run_tests",
            "Run explicit vectors, or the built-in ALU contract when omitted; not formal proof",
            json!({"vectors":vectors}),
            vec![],
        ),
        (
            "revision.diff",
            "Compare the previous document with the current one",
            empty.clone(),
            vec![],
        ),
        (
            "experiment.create",
            "Fork 1–8 isolated candidates; empty operations creates a baseline",
            json!({"expected_revision":revision,"name":{"type":"string","maxLength":120},"variants":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"object","required":["name","operations"],"properties":{"name":{"type":"string"},"operations":{"type":"array","maxItems":512,"items":op}},"additionalProperties":false}}}),
            vec!["expected_revision", "name", "variants"],
        ),
        (
            "experiment.run",
            "Verify all candidates against the same contract",
            json!({"id":{"type":"string"},"vectors":vectors}),
            vec!["id"],
        ),
        (
            "experiment.compare",
            "Read candidate results without invented PPA metrics",
            json!({"id":{"type":"string"}}),
            vec!["id"],
        ),
        (
            "experiment.get_status",
            "Read experiment status",
            json!({"id":{"type":"string"}}),
            vec!["id"],
        ),
        (
            "tools.status",
            "Detect installed EDA tools",
            empty.clone(),
            vec![],
        ),
        (
            "verify.rtl",
            "Compare generated RTL with Rust and the vector contract using Icarus or Verilator; sampled tests, not formal proof",
            json!({"expected_revision":revision,"vectors":vectors,"engine":{"enum":["icarus","verilator"]}}),
            vec!["expected_revision"],
        ),
        (
            "synthesis.run",
            "Run Yosys using generated RTL, bounded time, and fixed arguments",
            empty,
            vec![],
        ),
    ];
    defs.into_iter().map(|(name,description,properties,required)|json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})).collect()
}

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
