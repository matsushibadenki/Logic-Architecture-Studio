use serde_json::{json, Value};
// Schemas are shared by MCP discovery and the GUI command service.
pub fn tools() -> Vec<Value> {
    let revision = json!({"type":"integer","minimum":0});
    let identifier = json!({"type":"string","minLength":1,"maxLength":64});
    let kind = json!({"enum":["input","output","add","sub","and","nand","nor","xnor","or","xor","not","mux","register","constant","slice","concat","zero_extend","sign_extend","module"]});
    let module_port = json!({"type":"object","required":["name","width","signed"],"additionalProperties":false,"properties":{"name":identifier,"width":{"type":"integer","minimum":1,"maximum":64},"signed":{"type":"boolean"}}});
    let config = json!({"oneOf":[
        {"type":"object","required":["type","definition","inputs","outputs"],"additionalProperties":false,"properties":{"type":{"const":"module"},"definition":identifier,"inputs":{"type":"array","maxItems":16,"items":module_port},"outputs":{"type":"array","minItems":1,"maxItems":16,"items":module_port}}},
        {"type":"object","required":["type","input_width"],"additionalProperties":false,"properties":{"type":{"enum":["zero_extend","sign_extend"]},"input_width":{"type":"integer","minimum":1,"maximum":64}}},
        {"type":"object","required":["type"],"additionalProperties":false,"properties":{"type":{"const":"none"}}},
        {"type":"object","required":["type","value"],"additionalProperties":false,"properties":{"type":{"const":"constant"},"value":{"type":"string","minLength":1,"maxLength":20}}},
        {"type":"object","required":["type","input_width","lsb"],"additionalProperties":false,"properties":{"type":{"const":"slice"},"input_width":{"type":"integer","minimum":1,"maximum":64},"lsb":{"type":"integer","minimum":0,"maximum":63}}},
        {"type":"object","required":["type","high_width","low_width"],"additionalProperties":false,"properties":{"type":{"const":"concat"},"high_width":{"type":"integer","minimum":1,"maximum":63},"low_width":{"type":"integer","minimum":1,"maximum":63}}}
    ]});
    let node = json!({"type":"object","required":["id","name","kind","width","x","y"],"additionalProperties":false,"properties":{"id":identifier,"name":identifier,"kind":kind,"signed":{"type":"boolean"},"config":config,"width":{"type":"integer","minimum":1,"maximum":64},"x":{"type":"number","minimum":-100000,"maximum":100000},"y":{"type":"number","minimum":-100000,"maximum":100000}}});
    let edge = json!({"type":"object","required":["id","from","to","port"],"additionalProperties":false,"properties":{"id":identifier,"from":identifier,"to":identifier,"port":identifier,"source_port":identifier}});
    let op = json!({"oneOf":[
        {"type":"object","required":["op","node"],"additionalProperties":false,"properties":{"op":{"enum":["add_node"]},"node":node}},
        {"type":"object","required":["op","id"],"additionalProperties":false,"properties":{"op":{"enum":["remove_node","disconnect"]},"id":identifier}},
        {"type":"object","required":["op","id"],"additionalProperties":false,"properties":{"op":{"enum":["update_node"]},"id":identifier,"name":{"type":["string","null"],"minLength":1,"maxLength":64},"kind":{"enum":["input","output","add","sub","and","nand","nor","xnor","or","xor","not","mux","register","constant","slice","concat","zero_extend","sign_extend","module",null]},"signed":{"type":"boolean"},"config":config,"width":{"type":["integer","null"],"minimum":1,"maximum":64},"x":{"type":["number","null"],"minimum":-100000,"maximum":100000},"y":{"type":["number","null"],"minimum":-100000,"maximum":100000}}},
        {"type":"object","required":["op","edge"],"additionalProperties":false,"properties":{"op":{"enum":["connect"]},"edge":edge}}
    ]});
    let vectors = json!({"type":"array","minItems":1,"maxItems":4096,"items":{"type":"object","required":["inputs","expected"],"additionalProperties":false,"properties":{"inputs":{"type":"object","additionalProperties":{"type":"string"}},"expected":{"type":"object","additionalProperties":{"type":"string"}}}}});
    let patch = json!({"expected_revision":revision,"label":{"type":"string","maxLength":160},"operations":{"type":"array","items":op,"minItems":1,"maxItems":512}});
    let mut preview = patch.clone();
    preview["compact"] = json!({"type":"boolean"});
    let empty = json!({});
    let defs = vec![
        ("module.edit","Edit a shared module definition with a fixed interface",json!({"expected_revision":revision,"definition":identifier,"operations":{"type":"array","items":op,"minItems":1,"maxItems":512}}),vec!["expected_revision","definition","operations"]),
        ("module.group","Create a reusable module from selected processing nodes, preserving top-level IO",json!({"expected_revision":revision,"name":identifier,"ids":{"type":"array","minItems":1,"maxItems":512,"items":identifier}}),vec!["expected_revision","name","ids"]),
        ("module.instantiate","Place an independent instance of a reusable module",json!({"expected_revision":revision,"name":identifier,"x":{"type":"number","minimum":-100000,"maximum":100000},"y":{"type":"number","minimum":-100000,"maximum":100000}}),vec!["expected_revision","name","x","y"]),
        ("module.explode","Expand an instance into editable nodes, preserving connections and its reusable definition",json!({"expected_revision":revision,"id":identifier}),vec!["expected_revision","id"]),
        ("design.set_signed", "Set signedness on the connected data path atomically; extension inputs and selectors are boundaries", json!({"expected_revision":revision,"id":identifier,"signed":{"type":"boolean"}}), vec!["expected_revision","id","signed"]),
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
            preview,
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
            "view.select_nodes",
            "Select up to 512 nodes in all views; empty IDs clears selection",
            json!({"expected_revision":revision,"ids":{"type":"array","maxItems":512,"items":identifier}}),
            vec!["expected_revision","ids"],
        ),
        (
            "rtl.generate",
            "Generate synthesizable Verilog from the validated IR",
            empty.clone(),
            vec![],
        ),
        ("testbench.set", "Save a revision/version-guarded stimulus table without changing circuit history", json!({"expected_revision":revision,"expected_version":revision,"rows":{"type":"array","minItems":1,"maxItems":256,"items":{"type":"object","required":["reset","inputs","expected"],"additionalProperties":false,"properties":{"reset":{"type":"boolean"},"inputs":{"type":"object","additionalProperties":{"type":"string","maxLength":20}},"expected":{"type":"object","additionalProperties":{"type":"string","maxLength":160}}}}}}),vec!["expected_revision","expected_version","rows"]),
        ("testbench.import_csv", "Atomically import cycle,reset,in.<name>,expect.<name> CSV columns", json!({"expected_revision":revision,"expected_version":revision,"source":{"type":"string","maxLength":262144}}),vec!["expected_revision","expected_version","source"]),
        ("testbench.export_csv", "Export the saved stimulus table as CSV",json!({"expected_revision":revision,"expected_version":revision}),vec!["expected_revision","expected_version"]),
        ("simulation.rtl_testbench", "Execute saved stimuli using generated RTL, compare all nodes with Rust and output expectations, retain native simulator VCD",json!({"expected_revision":revision,"expected_version":revision,"engine":{"enum":["icarus","verilator"]}}),vec!["expected_revision","expected_version","engine"]),
        ("simulation.testbench", "Run saved stimuli with exact output expectations and shared trace/VCD",json!({"expected_revision":revision,"expected_version":revision}),vec!["expected_revision","expected_version"]),
        (
            "simulation.cycles",
            "Simulate rising clock edges with synchronous reset and return register and output samples",
            json!({"expected_revision":revision,"cycles":{"type":"array","minItems":1,"maxItems":256,"items":{"type":"object","required":["reset","inputs"],"additionalProperties":false,"properties":{"reset":{"type":"boolean"},"inputs":{"type":"object","additionalProperties":{"type":"string"}}}}}}),
            vec!["expected_revision", "cycles"],
        ),
        (
            "simulation.vcd",
            "Export current two-state post-edge samples as VCD (synthetic 1 ns/sample, no timing simulation)",
            json!({"expected_revision":revision,"trace_id":{"type":"string"}}),
            vec!["expected_revision","trace_id"],
        ),
        (
            "simulation.state",
            "Read the current ephemeral simulation trace and selected post-edge annotation",
            json!({"cursor":{"type":["string","null"]}}),
            vec![],
        ),
        (
            "simulation.select",
            "Select a sample and number radix without changing the design",
            json!({"expected_revision":revision,"trace_id":{"type":"string"},"index":{"type":"integer","minimum":0,"maximum":255},"radix":{"enum":[10,16]}}),
            vec!["expected_revision","trace_id","index","radix"],
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
            json!({"vectors":vectors,"expected_revision":revision}),
            vec![],
        ),
        ("verify.get_result", "Read the last test report and whether it matches the current revision", empty.clone(), vec![]),
        (
            "revision.changes_since",
            "List retained revision summaries after a revision, or indicate that the client needs a fresh snapshot",
            json!({"since_revision":revision}),
            vec!["since_revision"],
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
            "verify.formal",
            "Prove generated combinational ALU RTL against the built-in contract for every binary input using Yosys SAT",
            json!({"expected_revision":revision}),
            vec!["expected_revision"],
        ),
        ("job.start", "Start an external verification or synthesis job",
         json!({"method":{"enum":["verify.rtl","verify.formal","simulation.rtl_testbench","synthesis.run"]},"args":{"type":"object"}}),
         vec!["method","args"]),
        ("job.status", "Read an EDA job status and its final result",
         json!({"id":{"type":"string","minLength":1,"maxLength":96}}), vec!["id"]),
        ("job.cancel", "Cancel an active EDA job and check its status until it has stopped",
         json!({"id":{"type":"string","minLength":1,"maxLength":96}}), vec!["id"]),
        (
            "synthesis.run",
            "Run Yosys using generated RTL, bounded time, and fixed arguments",
            json!({"expected_revision":revision}),
            vec![],
        ),
    ];
    defs.into_iter().map(|(name,description,properties,required)|json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})).collect()
}

pub use mcp_core::validate;
