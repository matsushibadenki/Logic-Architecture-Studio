//! Transport-neutral operation registry and adapter contract.
mod schema;
pub use schema::validate;

use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    /// A temporary view or simulation change permitted by the read-only host mode.
    Session,
    Write,
}

#[derive(Clone)]
pub struct Operation {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub access: Access,
}

impl Operation {
    pub fn new(
        name: &str,
        description: &str,
        properties: Value,
        required: &[&str],
        access: Access,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
            access,
        }
    }

    pub fn descriptor(&self) -> Value {
        let scope = match self.access {
            Access::Read => "read",
            Access::Session => "session",
            Access::Write => "write",
        };
        json!({
            "name":self.name, "description":self.description, "inputSchema":self.input_schema,
            "outputSchema":{"type":"object","properties":{"result":{}},"required":["result"]},
            "annotations":{"readOnlyHint":self.access == Access::Read},
            "scope":scope,
        })
    }
}

#[derive(Debug)]
pub struct AdapterError {
    pub code: &'static str,
    pub message: String,
}

impl AdapterError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub trait AppAdapter: Send + Sync {
    fn app_info(&self) -> Value;
    fn operations(&self) -> Vec<Operation>;
    fn invoke(&self, operation: &str, arguments: Value) -> Result<Value, AdapterError>;
}

/// Shared authorization and schema checks apply identically across transports.
pub struct ToolRegistry {
    adapter: Box<dyn AppAdapter>,
    operations: BTreeMap<String, Operation>,
    write_enabled: bool,
}

impl ToolRegistry {
    pub fn new(adapter: Box<dyn AppAdapter>, write_enabled: bool) -> Result<Self, AdapterError> {
        let mut operations = BTreeMap::new();
        for operation in adapter.operations().into_iter().chain([
            Operation::new(
                "core.get_app_info",
                "Read application identity and version",
                json!({}),
                &[],
                Access::Read,
            ),
            Operation::new(
                "core.get_capabilities",
                "Read registered operations and effective write access",
                json!({}),
                &[],
                Access::Read,
            ),
            Operation::new(
                "core.get_operation_help",
                "Read an operation schema and access requirements",
                json!({"name":{"type":"string","minLength":1,"maxLength":128}}),
                &["name"],
                Access::Read,
            ),
        ]) {
            if operations
                .insert(operation.name.clone(), operation)
                .is_some()
            {
                return Err(AdapterError::new(
                    "invalid_registry",
                    "Duplicate operation name",
                ));
            }
        }
        Ok(Self {
            adapter,
            operations,
            write_enabled,
        })
    }

    pub fn operations(&self) -> impl Iterator<Item = &Operation> {
        self.operations.values()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.operations.contains_key(name)
    }

    pub fn check(&self, name: &str, args: &Value) -> Result<Access, AdapterError> {
        let operation = self
            .operations
            .get(name)
            .ok_or_else(|| AdapterError::new("unknown_tool", "Unknown tool"))?;
        validate(&operation.input_schema, args)
            .map_err(|e| AdapterError::new("invalid_arguments", e))?;
        if operation.access == Access::Write && !self.write_enabled {
            return Err(AdapterError::new(
                "write_disabled",
                "Launch with --allow-write to authorize editing and jobs.",
            ));
        }
        Ok(operation.access)
    }

    pub fn invoke(&self, name: &str, args: Value) -> Result<Value, AdapterError> {
        self.check(name, &args)?;
        match name {
            "core.get_app_info" => Ok(self.adapter.app_info()),
            "core.get_capabilities" => Ok(json!({
                "write_enabled":self.write_enabled,
                "operations":self.operations.keys().collect::<Vec<_>>()
            })),
            "core.get_operation_help" => self
                .operations
                .get(args["name"].as_str().unwrap_or_default())
                .map(Operation::descriptor)
                .ok_or_else(|| AdapterError::new("unknown_operation", "Unknown operation")),
            _ => self.adapter.invoke(name, args),
        }
    }
}
