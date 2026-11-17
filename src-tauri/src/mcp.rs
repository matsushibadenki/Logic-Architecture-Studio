use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf, sync::Mutex};
use studio_service::mcp::DesktopServer;
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    publish: bool,
    session: bool,
    write: bool,
}

pub struct State {
    path: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    policy: Policy,
    server: Option<DesktopServer>,
}

impl State {
    pub fn new(root: &std::path::Path) -> Self {
        let path = root.join(".logic/mcp-policy.json");
        let policy = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(Inner {
                policy,
                server: None,
            }),
        }
    }

    pub fn stop(&self) {
        self.inner.lock().unwrap().server = None;
    }

    fn save(&self, policy: Policy) -> Result<(), String> {
        let temp = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec(&policy).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let result = file
            .write_all(&bytes)
            .and_then(|_| file.sync_all())
            .and_then(|_| fs::rename(&temp, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result.map_err(|e| e.to_string())
    }
}

fn authorize(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() == "preferences" {
        Ok(())
    } else {
        Err("preferences_required".into())
    }
}

#[derive(Serialize)]
pub struct Status {
    policy: Policy,
    endpoint: Option<String>,
    pending: Vec<studio_service::mcp::ApprovalSummary>,
}

#[tauri::command]
pub fn mcp_status(
    webview: tauri::Webview,
    state: tauri::State<'_, State>,
) -> Result<Status, String> {
    authorize(&webview)?;
    let inner = state.inner.lock().unwrap();
    let server = inner.server.as_ref().filter(|server| server.is_running());
    Ok(Status {
        policy: inner.policy,
        endpoint: server.map(|server| server.endpoint.clone()),
        pending: server.map(DesktopServer::pending).unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn mcp_configure(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    policy: Policy,
) -> Result<(), String> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<State>();
        let mut inner = state.inner.lock().unwrap();
        if inner.server.as_ref().is_some_and(DesktopServer::is_running) {
            return Err("mcp_stop_first".into());
        }
        state.save(policy)?;
        inner.policy = policy;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mcp_start(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<State>();
        let mut inner = state.inner.lock().unwrap();
        if inner.server.as_ref().is_some_and(DesktopServer::is_running) {
            return Ok(());
        }
        let policy = inner.policy;
        if !policy.publish {
            return Err("mcp_document_required".into());
        }
        inner.server = None;
        let root = app.state::<super::CoreRoot>().0.clone();
        let changed = app.clone();
        inner.server = Some(
            DesktopServer::start(root, policy.session, policy.write, move || {
                super::sync::wake(&changed);
                let _ = changed.emit("design-changed", ());
            })
            .map_err(|e| e.to_string())?,
        );
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mcp_stop(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || app.state::<State>().stop())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mcp_token(
    webview: tauri::Webview,
    state: tauri::State<'_, State>,
) -> Result<String, String> {
    authorize(&webview)?;
    let inner = state.inner.lock().unwrap();
    let server = inner
        .server
        .as_ref()
        .filter(|server| server.is_running())
        .ok_or("mcp_stopped")?;
    Ok(server.token.clone())
}

#[tauri::command]
pub async fn mcp_review(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    id: String,
) -> Result<serde_json::Value, String> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<State>();
        let inner = state.inner.lock().unwrap();
        let server = inner
            .server
            .as_ref()
            .filter(|server| server.is_running())
            .ok_or("mcp_stopped")?;
        server.review(&id).ok_or_else(|| "approval_expired".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mcp_decide(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    id: String,
    approve: bool,
) -> Result<(), String> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<State>();
        let inner = state.inner.lock().unwrap();
        let server = inner
            .server
            .as_ref()
            .filter(|server| server.is_running())
            .ok_or("mcp_stopped")?;
        server.decide(&id, approve)
    })
    .await
    .map_err(|e| e.to_string())?
}
