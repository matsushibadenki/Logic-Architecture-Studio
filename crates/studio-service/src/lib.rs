mod delta;
pub mod native_sync;
mod persistence;
pub mod recovery;
mod rtl_verification;
mod simulation;
use design_core::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub method: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default = "default_actor")]
    pub actor: String,
}
fn default_actor() -> String {
    "local".into()
}
pub fn workspace_root() -> PathBuf {
    std::env::var_os("LAS_WORKSPACE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap())
}
pub fn socket_path(root: &Path) -> PathBuf {
    let direct = root.join(".logic/studio.sock");
    if direct.as_os_str().len() <= 100 {
        return direct;
    }
    // macOS sockaddr_un paths are limited to 104 bytes. Application Support
    // paths routinely exceed this; keep long-workspace sockets in user temp.
    use sha2::{Digest, Sha256};
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let digest = format!(
        "{:x}",
        Sha256::digest(canonical.to_string_lossy().as_bytes())
    );
    std::env::temp_dir()
        .join(format!("las-{}", &digest[..16]))
        .join("core.sock")
}
#[derive(Clone)]
pub struct Service {
    pub state: Arc<Mutex<Workspace>>,
    sync: Arc<Mutex<delta::Journal>>,
    persistence: Arc<Mutex<persistence::Persistence>>,
    root: PathBuf,
    _lock: Arc<File>,
    rtl_job: Arc<Mutex<()>>,
    simulation: Arc<Mutex<simulation::Session>>,
}
impl Service {
    pub fn open(root: &Path) -> Result<Self, String> {
        let dir = root.join(".logic");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("owner.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "A Design Core already owns this workspace".to_string())?;
        let (persistence, state) = persistence::Persistence::open(root)?;
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            persistence: Arc::new(Mutex::new(persistence)),
            sync: Arc::new(Mutex::new(delta::Journal::new(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            )))),
            rtl_job: Arc::new(Mutex::new(())),
            simulation: Arc::new(Mutex::new(simulation::Session::default())),
            root: root.into(),
            _lock: Arc::new(lock),
        })
    }
    pub fn execute(&self, mut req: Request) -> Result<Value, String> {
        let compact = req.method == "view.command";
        if compact {
            let method = req.args["method"]
                .as_str()
                .ok_or("Missing view command")?
                .to_string();
            if ![
                "design.apply_patch",
                "design.undo",
                "design.redo",
                "project.import",
                "layout.auto_arrange",
                "view.focus_module",
                "view.select_nodes",
                "view.preferences",
            ]
            .contains(&method.as_str())
            {
                return Err("Unsupported view command".into());
            }
            req = Request {
                method,
                args: req.args["args"].clone(),
                actor: req.actor,
            };
        }
        if let Some(definition) = design_core::protocol::tools()
            .iter()
            .find(|t| t["name"] == req.method)
        {
            design_core::protocol::validate(&definition["inputSchema"], &req.args)?;
        }
        if req.method == "tools.status" {
            return Ok(tool_status());
        }
        if matches!(req.method.as_str(), "verify.rtl" | "verify.formal") {
            let _job = self
                .rtl_job
                .try_lock()
                .map_err(|_| "RTL verification already running")?;
            let (revision, document) = {
                let state = self.state.lock().map_err(|e| e.to_string())?;
                if req.args["expected_revision"].as_u64() != Some(state.revision) {
                    return Err("Revision conflict".into());
                }
                (state.revision, state.document.clone())
            };
            if req.method == "verify.formal" {
                return rtl_verification::prove_alu(&self.root, revision, &document);
            }
            let vectors = if req.args["vectors"].is_null() {
                design_core::alu_vectors(&document)?
            } else {
                serde_json::from_value(req.args["vectors"].clone()).map_err(|e| e.to_string())?
            };
            return match req.args["engine"].as_str().unwrap_or("icarus") {
                "icarus" => rtl_verification::run(&self.root, revision, &document, &vectors),
                "verilator" => {
                    rtl_verification::run_verilator(&self.root, revision, &document, &vectors)
                }
                _ => Err("Unsupported RTL engine".into()),
            };
        }
        if req.method == "synthesis.run" {
            let (revision, source) = {
                let state = self.state.lock().map_err(|e| e.to_string())?;
                (state.revision, state.document.rtl()?)
            };
            return synthesis(&self.root, revision, &source);
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if req.method.starts_with("simulation.") {
            return self.simulation.lock().map_err(|e| e.to_string())?.execute(
                &state,
                &req.method,
                &req.args,
            );
        }
        if req.method == "project.sync" {
            let journal = self.sync.lock().map_err(|e| e.to_string())?;
            let epoch = &journal.epoch;
            let cursor = journal.cursor();
            if req.args["cursor"].as_str() == Some(&cursor) {
                return Ok(json!({"cursor":cursor,"changed":false}));
            }
            if req.args["scope"] == "web" || req.args["scope"] == "native_delta" {
                if let Some(mut deltas) = req.args["cursor"].as_str().and_then(|c| journal.since(c))
                {
                    if req.args["scope"] == "native_delta" {
                        deltas = deltas.iter().map(native_sync::filter).collect();
                    }
                    return Ok(
                        json!({"cursor":cursor,"base":req.args["cursor"],"changed":true,"kind":"delta","deltas":deltas}),
                    );
                }
            }
            let snapshot = if req.args["scope"] == "native" || req.args["scope"] == "native_delta" {
                let mut snapshot = json!({"revision":state.revision,"selected":state.selected,"selection":state.selected_nodes(),
                    "locale":state.locale,"node_colors":state.node_colors,"panels":state.panels});
                // A service restart invalidates the document cache even when
                // the persisted revision happens to match the client's value.
                let same_session = req.args["cursor"]
                    .as_str()
                    .and_then(|c| c.rsplit_once(':'))
                    .is_some_and(|(e, _)| e == epoch);
                if req.args["scope"] == "native_delta"
                    || !same_session
                    || req.args["document_revision"].as_u64() != Some(state.revision)
                {
                    snapshot["document"] =
                        serde_json::to_value(&state.document).map_err(|e| e.to_string())?;
                }
                snapshot
            } else {
                state.snapshot()
            };
            return Ok(
                json!({"cursor":cursor,"changed":true,"kind":"snapshot","snapshot":snapshot}),
            );
        }
        if req.method == "project.inspect" || req.method == "design.get_ir" {
            return Ok(state.snapshot());
        }
        // Transactional clone: failed validation or disk writes never change the live document.
        let mut next = state.clone();
        let result = if compact {
            next.dispatch_view(&req.method, req.args.clone(), &req.actor)?
        } else {
            next.dispatch(&req.method, req.args.clone(), &req.actor)?
        };
        if ![
            "project.inspect",
            "project.export",
            "design.get_ir",
            "rtl.generate",
            "simulation.evaluate",
            "simulation.cycles",
            "design.propose_patch",
            "revision.diff",
            "experiment.compare",
            "experiment.get_status",
        ]
        .contains(&req.method.as_str())
        {
            let delta = delta::between(&state, &next);
            self.persistence.lock().map_err(|e| e.to_string())?.commit(
                &next,
                &req.method,
                &req.args,
                &req.actor,
            )?;
            *state = next;
            self.sync.lock().map_err(|e| e.to_string())?.publish(delta);
        }
        Ok(result)
    }
    #[cfg(unix)]
    pub fn listen(self) -> Result<(), String> {
        let socket = socket_path(&self.root);
        let parent = socket.parent().ok_or("Invalid socket directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        if fs::symlink_metadata(parent)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Socket directory must not be a symlink".into());
        }
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        if socket.exists() {
            fs::remove_file(&socket).map_err(|e| e.to_string())?;
        }
        let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        for stream in listener.incoming() {
            let mut stream = stream.map_err(|e| e.to_string())?;
            let service = self.clone();
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                let mut line = String::new();
                let result = (|| -> Result<Value, String> {
                    BufReader::new((&stream).take(1_048_577))
                        .read_line(&mut line)
                        .map_err(|e| e.to_string())?;
                    if line.len() > 1_048_576 {
                        return Err("Request exceeds 1 MiB".into());
                    }
                    let request = serde_json::from_str(&line).map_err(|e| e.to_string())?;
                    service.execute(request)
                })();
                let response = match result {
                    Ok(result) => json!({"ok":true,"result":result}),
                    Err(error) => json!({"ok":false,"error":error}),
                };
                let _ = writeln!(stream, "{response}");
            });
        }
        Ok(())
    }
}
#[cfg(unix)]
pub fn call(root: &Path, request: &Request) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket_path(root))
        .map_err(|e| format!("Start Design Core first (npm run core): {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(40)))
        .map_err(|e| e.to_string())?;
    writeln!(stream, "{}", serde_json::to_string(request).unwrap()).map_err(|e| e.to_string())?;
    let mut text = String::new();
    BufReader::new(stream.take(16 * 1024 * 1024))
        .read_line(&mut text)
        .map_err(|e| e.to_string())?;
    let response: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if response["ok"] == true {
        Ok(response["result"].clone())
    } else {
        Err(response["error"].as_str().unwrap_or("Core error").into())
    }
}
fn executable(name: &str) -> Option<PathBuf> {
    let mut directories: Vec<_> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    // Finder-launched macOS apps do not inherit a shell's Homebrew PATH.
    #[cfg(target_os = "macos")]
    directories.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    directories
        .into_iter()
        .map(|directory| directory.join(name))
        .find_map(|path| {
            if !path.is_file() {
                return None;
            }
            #[cfg(unix)]
            {
                if std::fs::metadata(&path).ok()?.permissions().mode() & 0o111 == 0 {
                    return None;
                }
            }
            path.canonicalize().ok()
        })
}
pub fn tool_status() -> Value {
    json!([
        "yosys",
        "verilator",
        "iverilog",
        "vvp",
        "openroad",
        "klayout"
    ]
    .into_iter()
    .map(|name| json!({"name":name,"installed":executable(name).is_some()}))
    .collect::<Vec<_>>())
}
fn synthesis(root: &Path, revision: u64, source: &str) -> Result<Value, String> {
    let binary =
        executable("yosys").ok_or("Yosys is not installed; no synthesis results were generated")?;
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = root.join(format!(".logic/runs/{revision}-{run}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("design.v"), source).map_err(|e| e.to_string())?;
    let log = File::create(dir.join("yosys.log")).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(binary)
        .current_dir(&dir)
        .args([
            "-p",
            "read_verilog design.v; synth; stat; write_json netlist.json",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Yosys timed out after 30 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut log = String::new();
    File::open(dir.join("yosys.log"))
        .map_err(|e| e.to_string())?
        .take(256 * 1024)
        .read_to_string(&mut log)
        .map_err(|e| e.to_string())?;
    let result = json!({"revision":revision,"success":status.success(),"engine":"Yosys","log":log,"artifact_directory":dir,"ppa":null});
    fs::write(
        dir.join("result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}

pub fn mcp_message(
    message: Value,
    initialized: &mut bool,
    write_enabled: bool,
    mut execute: impl FnMut(Request) -> Result<Value, String>,
) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message["method"].as_str().unwrap_or("");
    let id = id?;
    let response = (|| -> Result<Value, (i32, String)> {
        if message["jsonrpc"] != "2.0" {
            return Err((-32600, "JSON-RPC 2.0 required".into()));
        }
        if method == "initialize" {
            *initialized = true;
            return Ok(
                json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{},"resources":{},"prompts":{}},"serverInfo":{"name":"logic-architecture-studio","version":env!("CARGO_PKG_VERSION")},"instructions":"Inspect revision, propose patches, then apply with expected_revision. Writes require --allow-write. Candidate adoption is GUI-only. Legacy MCP 2025-11-25 transport."}),
            );
        }
        if !*initialized {
            return Err((-32002, "Initialize the MCP session first".into()));
        }
        let args = message["params"].clone();
        match method {
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools":design_core::protocol::tools()})),
            "tools/call" => {
                let name = args["name"]
                    .as_str()
                    .ok_or((-32602, "Missing tool name".into()))?;
                let definitions = design_core::protocol::tools();
                let definition = definitions
                    .iter()
                    .find(|t| t["name"] == name)
                    .ok_or((-32602, "Unknown tool".into()))?;
                let arguments = args.get("arguments").cloned().unwrap_or(json!({}));
                design_core::protocol::validate(&definition["inputSchema"], &arguments)
                    .map_err(|e| (-32602, e))?;
                let read_only = [
                    "project.inspect",
                    "project.export",
                    "design.get_ir",
                    "design.propose_patch",
                    "rtl.generate",
                    "simulation.evaluate",
                    "simulation.cycles",
                    "simulation.vcd",
                    "simulation.state",
                    "simulation.select",
                    "revision.diff",
                    "experiment.compare",
                    "experiment.get_status",
                    "tools.status",
                ];
                if !write_enabled && !read_only.contains(&name) {
                    return Ok(
                        json!({"isError":true,"content":[{"type":"text","text":"Write capability disabled. Launch with --allow-write to authorize editing and jobs."}]}),
                    );
                }
                let result = execute(Request {
                    method: name.into(),
                    args: arguments,
                    actor: "mcp".into(),
                });
                Ok(match result {
                    Ok(value) => {
                        json!({"isError":false,"content":[{"type":"text","text":value.to_string()}],"structuredContent":{"result":value}})
                    }
                    Err(error) => json!({"isError":true,"content":[{"type":"text","text":error}]}),
                })
            }
            "resources/list" => Ok(
                json!({"resources":[{"uri":"logic://workspace/ir","name":"Canonical design IR","mimeType":"application/json"}]}),
            ),
            "resources/read" => {
                if args["uri"] != "logic://workspace/ir" {
                    return Err((-32602, "Unknown resource".into()));
                }
                let state = execute(Request {
                    method: "design.get_ir".into(),
                    args: json!({}),
                    actor: "mcp".into(),
                })
                .map_err(|e| (-32000, e))?;
                Ok(
                    json!({"contents":[{"uri":"logic://workspace/ir","mimeType":"application/json","text":state.to_string()}]}),
                )
            }
            "prompts/list" => Ok(
                json!({"prompts":[{"name":"explore_architecture","description":"Revision-safe architecture exploration with independent verification"}]}),
            ),
            "prompts/get" => {
                if args["name"] != "explore_architecture" {
                    return Err((-32602, "Unknown prompt".into()));
                }
                Ok(
                    json!({"messages":[{"role":"user","content":{"type":"text","text":"Inspect the IR and its revision. Define an independent behavioral contract. Propose isolated candidates, run the same test vectors on every candidate, and report failures honestly. Never claim PPA improvements without actual synthesis and physical results. Leave adoption to the user."}}]}),
                )
            }
            _ => Err((-32601, "Method not found".into())),
        }
    })();
    Some(match response {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err((code, message)) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_persists_and_syncs_without_document_delta() {
        let root = std::env::temp_dir().join(format!("las-appearance-{}", std::process::id()));
        let request = |method: &str, args| Request {
            method: method.into(),
            args,
            actor: "gui".into(),
        };
        let service = Service::open(&root).unwrap();
        let web = service
            .execute(request("project.sync", json!({"scope":"web"})))
            .unwrap();
        service.execute(request("view.command", json!({"method":"view.preferences","args":{"locale":"ja","node_colors":{"add":"#123456"}}}))).unwrap();
        let delta = service
            .execute(request(
                "project.sync",
                json!({"scope":"web","cursor":web["cursor"]}),
            ))
            .unwrap();
        assert_eq!(delta["deltas"][0]["set"]["node_colors"]["add"], "#123456");
        assert!(delta["deltas"][0].get("document").is_none());
        drop(service);
        let reopened = Service::open(&root).unwrap();
        let state = reopened
            .execute(request("project.inspect", json!({})))
            .unwrap();
        assert_eq!(state["node_colors"]["add"], "#123456");
        assert_eq!(state["locale"], "ja");
        assert_eq!(state["revision"], 0);
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn view_mutations_return_small_ack_and_publish_delta() {
        let root = std::env::temp_dir().join(format!("las-view-{}", std::process::id()));
        let service = Service::open(&root).unwrap();
        let request = |method: &str, args| Request {
            method: method.into(),
            args,
            actor: "gui".into(),
        };
        let web = service
            .execute(request("project.sync", json!({"scope":"web"})))
            .unwrap();
        let ack = service
            .execute(request(
                "view.command",
                json!({"method":"view.preferences","args":{"locale":"zh"}}),
            ))
            .unwrap();
        assert_eq!(ack, json!({"revision":0,"committed":true}));
        let delta = service
            .execute(request(
                "project.sync",
                json!({"scope":"web","cursor":web["cursor"]}),
            ))
            .unwrap();
        assert_eq!(delta["deltas"][0]["set"]["locale"], "zh");
        assert!(delta["deltas"][0].get("document").is_none());
        assert!(service
            .execute(request(
                "view.command",
                json!({"method":"view.command","args":{}})
            ))
            .is_err());
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn conditional_sync_tracks_metadata_documents_and_service_restart() {
        let root = std::env::temp_dir().join(format!("las-sync-{}", std::process::id()));
        let service = Service::open(&root).unwrap();
        let request = |method: &str, args| Request {
            method: method.into(),
            args,
            actor: "test".into(),
        };
        let initial = service
            .execute(request("project.sync", json!({"scope":"native"})))
            .unwrap();
        assert!(initial["snapshot"]["document"].is_object());
        assert!(initial["snapshot"]["history"].is_null());
        let cursor = initial["cursor"].clone();
        let unchanged = service
            .execute(request(
                "project.sync",
                json!({"cursor":cursor,"scope":"native","document_revision":0}),
            ))
            .unwrap();
        assert_eq!(unchanged["changed"], false);
        assert!(unchanged.get("snapshot").is_none());
        assert!(serde_json::to_vec(&unchanged).unwrap().len() < 200);
        service
            .execute(request("view.preferences", json!({"locale":"zh"})))
            .unwrap();
        let meta = service
            .execute(request(
                "project.sync",
                json!({"cursor":cursor,"scope":"native","document_revision":0}),
            ))
            .unwrap();
        assert_eq!(meta["snapshot"]["locale"], "zh");
        assert!(meta["snapshot"].get("document").is_none());
        service
            .execute(request(
                "layout.auto_arrange",
                json!({"expected_revision":0}),
            ))
            .unwrap();
        let changed = service
            .execute(request(
                "project.sync",
                json!({"cursor":meta["cursor"],"scope":"native","document_revision":0}),
            ))
            .unwrap();
        assert_eq!(changed["snapshot"]["revision"], 1);
        assert!(changed["snapshot"]["document"].is_object());
        let ui = service.execute(request("project.sync", json!({}))).unwrap();
        assert_eq!(
            ui["snapshot"],
            service
                .execute(request("project.inspect", json!({})))
                .unwrap()
        );
        drop(service);
        let restarted = Service::open(&root).unwrap();
        let restored = restarted
            .execute(request(
                "project.sync",
                json!({"cursor":changed["cursor"],"scope":"native","document_revision":1}),
            ))
            .unwrap();
        assert_ne!(restored["cursor"], changed["cursor"]);
        assert!(restored["snapshot"]["document"].is_object());
        drop(restarted);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mcp_permissions_and_discovery() {
        let mut init = false;
        let response = mcp_message(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
            &mut init,
            false,
            |_| unreachable!(),
        )
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
        let result=mcp_message(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"design.undo","arguments":{"expected_revision":0}}}),&mut init,false,|_|panic!("read-only must not execute writes")).unwrap();
        assert_eq!(result["result"]["isError"], true);
    }
    #[test]
    fn failed_move_log_write_does_not_publish_state_or_cursor() {
        let root = std::env::temp_dir().join(format!("las-wal-fail-{}", std::process::id()));
        let service = Service::open(&root).unwrap();
        let request = |method: &str, args| Request {
            method: method.into(),
            args,
            actor: "test".into(),
        };
        let before = service
            .execute(request("project.inspect", json!({})))
            .unwrap();
        let cursor = service
            .execute(request("project.sync", json!({"scope":"native_delta"})))
            .unwrap()["cursor"]
            .clone();
        let wal = root.join(".logic/workspace.wal");
        let backup = root.join(".logic/workspace.wal.backup");
        fs::rename(&wal, &backup).unwrap();
        fs::create_dir(&wal).unwrap();
        assert!(service.execute(request("view.command", json!({"method":"design.apply_patch","args":{
            "expected_revision":0,"label":"move","operations":[{"op":"update_node","id":"add","x":777}]
        }}))).is_err());
        assert_eq!(
            service
                .execute(request("project.inspect", json!({})))
                .unwrap(),
            before
        );
        assert_eq!(
            service
                .execute(request(
                    "project.sync",
                    json!({"scope":"native_delta","cursor":cursor})
                ))
                .unwrap()["changed"],
            false
        );
        fs::remove_dir(&wal).unwrap();
        fs::rename(backup, wal).unwrap();
        drop(service);
        let reopened = Service::open(&root).unwrap();
        assert_eq!(
            reopened
                .execute(request("project.inspect", json!({})))
                .unwrap(),
            before
        );
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_disk_write_rolls_back() {
        let root = std::env::temp_dir().join(format!("las-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let service = Service::open(&root).unwrap();
        let cursor = service
            .execute(Request {
                method: "project.sync".into(),
                args: json!({}),
                actor: "test".into(),
            })
            .unwrap()["cursor"]
            .clone();
        fs::create_dir(root.join(".logic/workspace.tmp")).unwrap();
        assert!(service
            .execute(Request {
                method: "layout.auto_arrange".into(),
                args: json!({"expected_revision":0}),
                actor: "test".into()
            })
            .is_err());
        assert_eq!(service.state.lock().unwrap().revision, 0);
        let unchanged = service
            .execute(Request {
                method: "project.sync".into(),
                args: json!({"cursor":cursor}),
                actor: "test".into(),
            })
            .unwrap();
        assert_eq!(unchanged["changed"], false);
        let _ = fs::remove_dir_all(root);
    }
}
