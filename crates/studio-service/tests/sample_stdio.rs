use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    id: u64,
}

impl Client {
    fn start(writable: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_logic-mcp"));
        command
            .arg("--sample")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if writable {
            command.arg("--allow-write");
        }
        let mut child = command.spawn().unwrap();
        let mut client = Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            id: 0,
        };
        let discovery = client.request("server/discover", json!({}));
        assert!(discovery["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!("2026-07-28")));
        client
    }

    fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.id += 1;
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name":"sample-test","version":"1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], self.id);
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name":name,"arguments":arguments}))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn standalone_sample_discovery_validation_and_crud_over_stdio() {
    let mut read_only = Client::start(false);
    let tools = read_only.request("tools/list", json!({}));
    let tools = tools["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "sample.list_objects"));
    assert!(tools.iter().all(|t| t["name"] != "design.apply_patch"));
    assert_eq!(
        read_only.call(
            "sample.create_object",
            json!({"expected_revision":0,"name":"Blocked"})
        )["structuredContent"]["error"]["code"],
        "write_disabled"
    );
    assert_eq!(
        read_only.call("core.get_context", json!({}))["structuredContent"]["result"]["document"]
            ["revision"],
        0
    );

    let mut writer = Client::start(true);
    let info = writer.call("core.get_app_info", json!({}));
    assert_eq!(info["structuredContent"]["result"]["mode"], "sample");
    let create = writer.call(
        "sample.create_object",
        json!({"name":"Second","expected_revision":0}),
    );
    let id = &create["structuredContent"]["result"]["id"];
    let first_page = writer.call(
        "core.get_document_state",
        json!({"document_id":"sample","limit":1}),
    );
    assert_eq!(
        first_page["structuredContent"]["result"]["objects"]["next_offset"],
        1
    );
    assert_eq!(
        writer.call(
            "sample.update_object",
            json!({"id":id,"name":"Updated","expected_revision":1})
        )["structuredContent"]["result"]["revision"],
        2
    );
    for (rev, confirm, code) in [
        (1, true, "revision_conflict"),
        (2, false, "invalid_arguments"),
    ] {
        let denied = writer.call(
            "sample.delete_object",
            json!({"id":id,"expected_revision":rev,"confirm":confirm}),
        );
        assert_eq!(denied["isError"], true);
        assert_eq!(denied["structuredContent"]["error"]["code"], code);
    }
    let updated = writer.call("sample.list_objects", json!({"offset":1,"limit":1}));
    assert_eq!(
        updated["structuredContent"]["result"]["objects"]["items"][0]["name"],
        "Updated"
    );
    assert_eq!(
        writer.call(
            "sample.delete_object",
            json!({"id":id,"expected_revision":2,"confirm":true})
        )["structuredContent"]["result"]["revision"],
        3
    );
    let remaining = writer.call("sample.list_objects", json!({}));
    assert_eq!(
        remaining["structuredContent"]["result"]["objects"]["total"],
        1
    );
}
