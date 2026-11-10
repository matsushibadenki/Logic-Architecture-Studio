use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
#[ignore = "Requires local Unix sockets and child processes"]
fn mcp_and_gui_share_revisions_and_persist() {
    let root = std::env::temp_dir().join(format!(
        "las-interop-{}-{}",
        std::process::id(),
        "long-workspace-path".repeat(6)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let start = || {
        Guard(
            Command::new(env!("CARGO_BIN_EXE_logic-service"))
                .env("LAS_WORKSPACE", &root)
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    };
    let mut service = start();
    let call = |method: &str, args: Value| {
        studio_service::call(
            &root,
            &studio_service::Request {
                method: method.into(),
                args,
                actor: "gui".into(),
            },
        )
    };
    for _ in 0..100 {
        if call("project.inspect", json!({})).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let initial = call("project.inspect", json!({})).unwrap();
    let sync = call("project.sync", json!({"scope":"native"})).unwrap();
    let web = call("project.sync", json!({"scope":"web"})).unwrap();
    assert_eq!(
        call("project.sync", json!({"cursor":sync["cursor"]})).unwrap()["changed"],
        false
    );
    assert_eq!(initial["revision"], 0);
    let mut mcp = Guard(
        Command::new(env!("CARGO_BIN_EXE_logic-mcp"))
            .arg("--allow-write")
            .env("LAS_WORKSPACE", &root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut input = mcp.0.stdin.take().unwrap();
    let mut output = BufReader::new(mcp.0.stdout.take().unwrap());
    let mut request = |method: &str, params: Value| {
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
        )
        .unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    assert_eq!(
        request(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}})
        )["result"]["protocolVersion"],
        "2025-11-25"
    );
    assert!(
        request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .len()
            >= 18
    );
    let patch = json!({"expected_revision":0,"label":"MCP rename","operations":[{"op":"update_node","id":"add","name":"sum"}]});
    assert_eq!(
        request(
            "tools/call",
            json!({"name":"design.propose_patch","arguments":patch})
        )["result"]["isError"],
        false
    );
    assert_eq!(call("project.inspect", json!({})).unwrap()["revision"], 0);
    assert_eq!(
        request(
            "tools/call",
            json!({"name":"design.apply_patch","arguments":patch})
        )["result"]["isError"],
        false
    );
    let updated = call("project.inspect", json!({})).unwrap();
    let web_delta = call(
        "project.sync",
        json!({"scope":"web","cursor":web["cursor"]}),
    )
    .unwrap();
    assert_eq!(web_delta["kind"], "delta");
    assert_eq!(
        web_delta["deltas"][0]["document"]["nodes"]["upsert"][0]["name"],
        "sum"
    );
    assert!(web_delta.get("snapshot").is_none());
    let synced = call(
        "project.sync",
        json!({"scope":"native","cursor":sync["cursor"],"document_revision":0}),
    )
    .unwrap();
    assert_eq!(synced["snapshot"]["document"], updated["document"]);
    assert_eq!(synced["snapshot"]["revision"], 1);
    assert_eq!(updated["revision"], 1);
    assert!(updated["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["name"] == "sum"));
    assert!(call("design.apply_patch", patch)
        .unwrap_err()
        .contains("conflict"));
    assert_eq!(call("verify.run_tests", json!({})).unwrap()["passed"], 320);
    let moved = call("view.command", json!({"method":"design.apply_patch","args":{
        "expected_revision":1,"label":"durable move","operations":[{"op":"update_node","id":"add","x":777,"y":88}]
    }})).unwrap();
    assert_eq!(moved["revision"], 2);
    assert!(
        std::fs::metadata(root.join(".logic/workspace.wal"))
            .unwrap()
            .len()
            > 0
    );
    service.0.kill().unwrap();
    service.0.wait().unwrap();
    service = start();
    for _ in 0..100 {
        if call("project.inspect", json!({})).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(call("project.inspect", json!({})).unwrap()["revision"], 2);
    let recovered = call("project.inspect", json!({})).unwrap();
    assert_eq!(
        recovered["document"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "add")
            .unwrap()["x"],
        777.0
    );
    let restarted_sync = call(
        "project.sync",
        json!({"scope":"native","cursor":synced["cursor"],"document_revision":1}),
    )
    .unwrap();
    assert!(restarted_sync["snapshot"]["document"].is_object());
    assert_ne!(restarted_sync["cursor"], synced["cursor"]);
    let web_reset = call(
        "project.sync",
        json!({"scope":"web","cursor":web_delta["cursor"]}),
    )
    .unwrap();
    assert_eq!(web_reset["kind"], "snapshot");
    assert_eq!(
        call("design.undo", json!({"expected_revision":2})).unwrap()["revision"],
        3
    );
    drop(service);
    drop(mcp);
    std::fs::remove_dir_all(root).unwrap();
}
