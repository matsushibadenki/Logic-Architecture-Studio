use crate::Request;
use mcp_core::{Access, AdapterError, AppAdapter, Operation};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DOCUMENT_ID: &str = "workspace";
const SAMPLE_ID: &str = "sample";

pub struct StudioAdapter(pub PathBuf);

fn domain_error(message: String) -> AdapterError {
    let code = if message == "Revision conflict" || message.starts_with("Revision conflict:") {
        "revision_conflict"
    } else if message.starts_with("Start Design Core first") {
        "app_not_connected"
    } else {
        "tool_failed"
    };
    AdapterError::new(code, message)
}

fn document_operations() -> [Operation; 3] {
    [
        Operation::new(
            "core.list_documents",
            "List the document available to this connection",
            json!({}),
            &[],
            Access::Read,
        ),
        Operation::new(
            "core.get_context",
            "Read current document, selection and revision",
            json!({}),
            &[],
            Access::Read,
        ),
        Operation::new(
            "core.get_document_state",
            "Read a bounded page of document entities; offset is zero-based",
            json!({"document_id":{"type":"string","minLength":1,"maxLength":64},
                "offset":{"type":"integer","minimum":0,"maximum":10000},
                "limit":{"type":"integer","minimum":1,"maximum":100}}),
            &["document_id"],
            Access::Read,
        ),
    ]
}

fn page<'a>(args: &Value, entries: impl IntoIterator<Item = &'a Value>, total: usize) -> Value {
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(50) as usize;
    let items: Vec<_> = entries.into_iter().skip(offset).take(limit).collect();
    json!({"offset":offset,"total":total,"items":items,
        "next_offset": (offset + items.len() < total).then_some(offset + items.len())})
}

fn document_id(args: &Value, expected: &str) -> Result<(), AdapterError> {
    if args["document_id"] == expected {
        Ok(())
    } else {
        Err(AdapterError::new(
            "document_not_found",
            "Unknown document ID",
        ))
    }
}

impl AppAdapter for StudioAdapter {
    fn app_info(&self) -> Value {
        json!({"name":"logic-architecture-studio","version":VERSION,"mode":"workspace"})
    }

    fn operations(&self) -> Vec<Operation> {
        design_core::protocol::tools()
            .into_iter()
            .map(|tool| {
                let name = tool["name"].as_str().expect("static tool name");
                let access = match name {
                    "project.inspect"
                    | "project.export"
                    | "design.get_ir"
                    | "design.propose_patch"
                    | "rtl.generate"
                    | "testbench.export_csv"
                    | "simulation.vcd"
                    | "simulation.state"
                    | "revision.list_snapshots"
                    | "revision.diff"
                    | "revision.changes_since"
                    | "experiment.compare"
                    | "experiment.get_status"
                    | "tools.status"
                    | "verify.get_result"
                    | "job.status" => Access::Read,
                    "simulation.evaluate"
                    | "simulation.cycles"
                    | "simulation.testbench"
                    | "simulation.select" => Access::Session,
                    _ => Access::Write,
                };
                Operation {
                    name: name.into(),
                    description: tool["description"].as_str().unwrap_or_default().into(),
                    input_schema: tool["inputSchema"].clone(),
                    access,
                }
            })
            .chain(document_operations())
            .collect()
    }

    fn invoke(&self, name: &str, args: Value) -> Result<Value, AdapterError> {
        let invoke = |method: &str, args| {
            crate::call(
                &self.0,
                &Request {
                    method: method.into(),
                    args,
                    actor: "mcp".into(),
                },
            )
            .map_err(domain_error)
        };
        match name {
            "core.list_documents" | "core.get_context" | "core.get_document_state" => {
                if name == "core.get_document_state" {
                    document_id(&args, DOCUMENT_ID)?;
                }
                let state = invoke("project.inspect", json!({}))?;
                let document = &state["document"];
                let info =
                    json!({"id":DOCUMENT_ID,"name":document["name"],"revision":state["revision"]});
                match name {
                    "core.list_documents" => Ok(json!({"documents":[info]})),
                    "core.get_context" => {
                        Ok(json!({"document":info,"selection":state["selection"]}))
                    }
                    _ => {
                        let nodes = document["nodes"].as_array().ok_or_else(|| {
                            AdapterError::new("tool_failed", "Missing document nodes")
                        })?;
                        let edges = document["edges"].as_array().ok_or_else(|| {
                            AdapterError::new("tool_failed", "Missing document edges")
                        })?;
                        Ok(
                            json!({"document":info,"nodes":page(&args, nodes, nodes.len()),"edges":page(&args, edges, edges.len())}),
                        )
                    }
                }
            }
            _ => invoke(name, args),
        }
    }
}

pub struct SampleAdapter(Mutex<Sample>);

struct Sample {
    revision: u64,
    next_id: u64,
    objects: BTreeMap<u64, Value>,
}

impl Default for SampleAdapter {
    fn default() -> Self {
        Self(Mutex::new(Sample {
            revision: 0,
            next_id: 2,
            objects: BTreeMap::from([(1, json!({"id":1,"name":"Example"}))]),
        }))
    }
}

impl AppAdapter for SampleAdapter {
    fn app_info(&self) -> Value {
        json!({"name":"logic-studio-sample","version":VERSION,"mode":"sample"})
    }

    fn operations(&self) -> Vec<Operation> {
        let revision = json!({"type":"integer","minimum":0});
        let id = json!({"type":"integer","minimum":1});
        let name = json!({"type":"string","minLength":1,"maxLength":80});
        document_operations().into_iter().chain([
            Operation::new("sample.list_objects", "Read a bounded page of sample objects",
                json!({"offset":{"type":"integer","minimum":0,"maximum":10000},
                    "limit":{"type":"integer","minimum":1,"maximum":100}}), &[], Access::Read),
            Operation::new("sample.create_object", "Create one object in the in-memory sample",
                json!({"name":name,"expected_revision":revision}), &["name","expected_revision"], Access::Write),
            Operation::new("sample.update_object", "Rename one sample object",
                json!({"id":id,"name":name,"expected_revision":revision}), &["id","name","expected_revision"], Access::Write),
            Operation::new("sample.delete_object", "Delete one sample object with explicit confirmation",
                json!({"id":id,"expected_revision":revision,"confirm":{"type":"boolean","const":true}}),
                &["id","expected_revision","confirm"], Access::Write),
        ]).collect()
    }

    fn invoke(&self, operation: &str, args: Value) -> Result<Value, AdapterError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| AdapterError::new("tool_failed", "Sample state unavailable"))?;
        let info = json!({"id":SAMPLE_ID,"name":"Sample document","revision":state.revision});
        match operation {
            "core.list_documents" => return Ok(json!({"documents":[info]})),
            "core.get_context" => return Ok(json!({"document":info,"selection":[]})),
            "core.get_document_state" => {
                document_id(&args, SAMPLE_ID)?;
                return Ok(
                    json!({"document":info,"objects":page(&args, state.objects.values(), state.objects.len())}),
                );
            }
            "sample.list_objects" => {
                return Ok(
                    json!({"revision":state.revision,"objects":page(&args, state.objects.values(), state.objects.len())}),
                );
            }
            _ => {}
        }
        if args["expected_revision"].as_u64() != Some(state.revision) {
            return Err(AdapterError::new(
                "revision_conflict",
                format!("Sample is at revision {}", state.revision),
            ));
        }
        let id = match operation {
            "sample.create_object" if state.objects.len() < 1000 => {
                let id = state.next_id;
                state.next_id += 1;
                state
                    .objects
                    .insert(id, json!({"id":id,"name":args["name"]}));
                id
            }
            "sample.create_object" => {
                return Err(AdapterError::new(
                    "sample_limit",
                    "Sample is limited to 1000 objects",
                ))
            }
            "sample.update_object" | "sample.delete_object" => {
                let id = args["id"].as_u64().unwrap_or_default();
                let object = state.objects.get_mut(&id).ok_or_else(|| {
                    AdapterError::new("object_not_found", "Unknown sample object")
                })?;
                if operation == "sample.update_object" {
                    object["name"] = args["name"].clone();
                } else {
                    if args["confirm"] != true {
                        return Err(AdapterError::new(
                            "invalid_arguments",
                            "Deletion requires confirm: true",
                        ));
                    }
                    state.objects.remove(&id);
                }
                id
            }
            _ => {
                return Err(AdapterError::new(
                    "unknown_tool",
                    "Unknown sample operation",
                ))
            }
        };
        let previous = state.revision;
        state.revision += 1;
        Ok(json!({"id":id,"previous_revision":previous,"revision":state.revision,"can_undo":false}))
    }
}
