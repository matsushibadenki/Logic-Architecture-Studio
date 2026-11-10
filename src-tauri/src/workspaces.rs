use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema: u32,
    path: PathBuf,
}
pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            path: directory.join("workspace-selection.json"),
        }
    }
    pub fn load(&self) -> Result<Option<PathBuf>, String> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let selected: Selection = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid workspace selection: {e}"))?;
        if selected.schema != 1 || !selected.path.is_absolute() {
            return Err("Invalid workspace selection".into());
        }
        if !selected.path.is_dir() {
            return Err(format!(
                "Workspace unavailable: {}",
                selected.path.display()
            ));
        }
        Ok(Some(selected.path))
    }
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let directory = self.path.parent().ok_or("Missing settings directory")?;
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let temp = directory.join(format!(
            "workspace-selection-{}-{nonce}.tmp",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(temp, &self.path).map_err(|e| e.to_string())?;
        File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        let root = validate(root)?;
        self.write(
            &serde_json::to_vec(&Selection {
                schema: 1,
                path: root,
            })
            .map_err(|e| e.to_string())?,
        )
    }
    fn previous(&self) -> Result<Option<Vec<u8>>, String> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn rollback(&self, old: Option<&[u8]>) -> Result<(), String> {
        if let Some(bytes) = old {
            self.write(bytes)
        } else {
            if !self.path.exists() {
                return Ok(());
            }
            fs::remove_file(&self.path).map_err(|e| e.to_string())?;
            File::open(self.path.parent().unwrap())
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())
        }
    }
}
fn switch_selection(
    store: &Store,
    selected: &Path,
    launch: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let old = store.previous()?;
    if let Err(error) = store.save(selected) {
        store
            .rollback(old.as_deref())
            .map_err(|e| format!("{error}; selection rollback failed: {e}"))?;
        return Err(error);
    }
    if let Err(error) = launch(selected) {
        store
            .rollback(old.as_deref())
            .map_err(|e| format!("{error}; selection rollback failed: {e}"))?;
        return Err(error);
    }
    Ok(())
}
fn validate(path: &Path) -> Result<PathBuf, String> {
    if !path.is_dir() {
        return Err("Select a workspace directory".into());
    }
    path.canonicalize().map_err(|e| e.to_string())
}
pub struct State {
    pub current: Option<PathBuf>,
    store: Mutex<Store>,
    picking: AtomicBool,
    pub error: Option<String>,
}
impl State {
    pub fn picker_guard(&self) -> Result<PickerGuard<'_>, String> {
        if self.picking.swap(true, Ordering::AcqRel) {
            return Err("Workspace picker already open".into());
        }
        Ok(PickerGuard(&self.picking))
    }
    pub fn new(store: Store, current: Option<PathBuf>, error: Option<String>) -> Self {
        Self {
            current,
            store: Mutex::new(store),
            picking: AtomicBool::new(false),
            error,
        }
    }
}
pub struct PickerGuard<'a>(&'a AtomicBool);
impl Drop for PickerGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
pub fn show_start(app: &tauri::AppHandle) -> tauri::Result<()> {
    tauri::WebviewWindowBuilder::new(
        app,
        "workspace-start",
        tauri::WebviewUrl::App("index.html?workspace_start=1".into()),
    )
    .title("Logic Architecture Studio")
    .inner_size(680., 420.)
    .min_inner_size(520., 360.)
    .build()?;
    Ok(())
}
fn authorize(webview: &tauri::Webview) -> Result<(), String> {
    if !["controls", "recovery", "workspace-start"].contains(&webview.label()) {
        return Err("Workspace controls required".into());
    }
    Ok(())
}
#[tauri::command]
pub fn workspace_info(
    webview: tauri::Webview,
    state: tauri::State<'_, State>,
) -> Result<serde_json::Value, String> {
    authorize(&webview)?;
    Ok(serde_json::json!({"path":state.current,"error":state.error}))
}
pub fn spawn(root: &Path) -> Result<(), String> {
    let mut process =
        std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    process
        .arg("--workspace")
        .arg(root)
        .env("LAS_WORKSPACE", root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        process.process_group(0);
    }
    process.spawn().map_err(|e| e.to_string())?;
    Ok(())
}
async fn pick_directory(
    webview: &tauri::Webview,
    current: Option<&Path>,
    locale: &str,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        let title = match locale {
            "en" => "Open workspace folder",
            "ja" => "ワークスペースのフォルダーを開く",
            "zh" => "打开工作区文件夹",
            _ => return Err("Invalid locale".into()),
        };
        let parent = webview.window();
        let mut dialog = rfd::AsyncFileDialog::new()
            .set_title(title)
            .set_parent(&parent)
            .set_can_create_directories(true);
        if let Some(root) = current {
            dialog = dialog.set_directory(root);
        }
        Ok(dialog
            .pick_folder()
            .await
            .map(|file| file.path().to_path_buf()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (webview, current, locale);
        Err("Native workspace picker is not supported on this platform yet".into())
    }
}
#[tauri::command]
pub async fn workspace_pick(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, State>,
    locale: String,
) -> Result<bool, String> {
    authorize(&webview)?;
    let _picking = state.picker_guard()?;
    let selected = pick_directory(&webview, state.current.as_deref(), &locale).await?;
    let Some(selected) = selected else {
        return Ok(false);
    };
    let selected = validate(&selected)?;
    if state.current.as_ref() == Some(&selected) {
        return Ok(false);
    }
    let store = state.store.lock().map_err(|e| e.to_string())?;
    switch_selection(&store, &selected, spawn)?;
    app.exit(0);
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_launch_rolls_back_saved_selection_and_never_touches_circuit_data() {
        let root =
            std::env::temp_dir().join(format!("las-workspace-rollback-{}", std::process::id()));
        fs::create_dir_all(root.join("first")).unwrap();
        fs::create_dir_all(root.join("second")).unwrap();
        let store = Store::new(root.join("settings"));
        assert!(switch_selection(
            &store,
            &root.join("second"),
            |_| Err("launch failed".into())
        )
        .is_err());
        assert!(store.load().unwrap().is_none());
        store.save(&root.join("first")).unwrap();
        let original = fs::read(&store.path).unwrap();
        assert!(switch_selection(
            &store,
            &root.join("second"),
            |_| Err("launch failed".into())
        )
        .is_err());
        assert_eq!(fs::read(&store.path).unwrap(), original);
        assert!(!root.join("first/.logic").exists());
        assert!(!root.join("second/.logic").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn selection_roundtrip_missing_and_bad_files_do_not_create_workspaces() {
        let root = std::env::temp_dir().join(format!("las-workspace-store-{}", std::process::id()));
        fs::create_dir_all(root.join("Project With Spaces")).unwrap();
        let store = Store::new(root.join("settings"));
        assert!(store.load().unwrap().is_none());
        store.save(&root.join("Project With Spaces")).unwrap();
        let selected = store.load().unwrap().unwrap();
        assert_eq!(
            selected,
            root.join("Project With Spaces").canonicalize().unwrap()
        );
        assert!(!selected.join(".logic").exists());
        fs::remove_dir(&selected).unwrap();
        assert!(store.load().is_err());
        assert!(!selected.exists());
        fs::write(&store.path, b"broken").unwrap();
        assert!(store.load().is_err());
        fs::write(&store.path, br#"{"schema":1,"path":"relative"}"#).unwrap();
        assert!(store.load().is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
