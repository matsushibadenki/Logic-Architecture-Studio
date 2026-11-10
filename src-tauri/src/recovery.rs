use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;
pub struct Context {
    root: PathBuf,
    error: String,
    layout: bool,
    backup: Mutex<Option<Arc<studio_service::recovery::ExternalCheckpoint>>>,
}
fn authorize(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "recovery" {
        return Err("Recovery window required".into());
    }
    Ok(())
}
pub fn show(
    app: &tauri::AppHandle,
    root: PathBuf,
    error: String,
    layout: bool,
) -> tauri::Result<()> {
    app.manage(Context {
        root,
        error,
        layout,
        backup: Mutex::new(None),
    });
    tauri::WebviewWindowBuilder::new(
        app,
        "recovery",
        tauri::WebviewUrl::App("index.html?recovery=1".into()),
    )
    .title("Logic Architecture Studio · Recovery")
    .inner_size(760., 620.)
    .min_inner_size(520., 440.)
    .build()?;
    Ok(())
}
#[tauri::command]
pub async fn recovery_status(
    window: tauri::WebviewWindow,
    context: tauri::State<'_, Context>,
) -> Result<serde_json::Value, String> {
    authorize(&window)?;
    let root = context.root.clone();
    let layout = context.layout;
    let error = context.error.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let status = studio_service::recovery::status(&root, layout)?;
        Ok(serde_json::json!({"root":root,"error":error,"status":status}))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn recovery_apply(
    window: tauri::WebviewWindow,
    context: tauri::State<'_, Context>,
    token: String,
    choice: String,
) -> Result<String, String> {
    authorize(&window)?;
    let root = context.root.clone();
    let layout = context.layout;
    let backup = context.backup.lock().map_err(|e| e.to_string())?.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if choice == "external" {
            if layout {
                return Err("Workspace recovery required".into());
            }
            return studio_service::recovery::restore_backup(
                &root,
                &token,
                backup.as_deref().ok_or("Choose a backup first")?,
            )
            .map(|path| path.to_string_lossy().into_owned());
        }
        studio_service::recovery::restore(&root, layout, &token, &choice)
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn recovery_choose_backup(
    window: tauri::WebviewWindow,
    context: tauri::State<'_, Context>,
    workspaces: tauri::State<'_, super::workspaces::State>,
    locale: String,
) -> Result<Option<serde_json::Value>, String> {
    authorize(&window)?;
    if context.layout {
        return Err("Workspace recovery required".into());
    }
    let _picking = workspaces.picker_guard()?;
    let path = backup_path(&window, &locale).await?;
    let Some(path) = path else { return Ok(None) };
    let backup =
        tauri::async_runtime::spawn_blocking(move || studio_service::recovery::read_backup(&path))
            .await
            .map_err(|e| e.to_string())??;
    let preview = backup.preview();
    *context.backup.lock().map_err(|e| e.to_string())? = Some(Arc::new(backup));
    Ok(Some(preview))
}
async fn backup_path(
    window: &tauri::WebviewWindow,
    locale: &str,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        let title = match locale {
            "en" => "Choose workspace checkpoint backup",
            "ja" => "ワークスペースのバックアップを選択",
            "zh" => "选择工作区检查点备份",
            _ => return Err("Invalid locale".into()),
        };
        Ok(rfd::AsyncFileDialog::new()
            .set_title(title)
            .set_parent(window)
            .add_filter("Workspace checkpoint", &["json"])
            .pick_file()
            .await
            .map(|file| file.path().to_path_buf()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, locale);
        Err("Native backup picker is not supported on this platform yet".into())
    }
}
pub fn workspace_argument(
    args: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Option<PathBuf>, String> {
    let mut args = args;
    let mut root = None;
    while let Some(arg) = args.next() {
        if arg == "--workspace" {
            if root.is_some() {
                return Err("Duplicate workspace argument".into());
            }
            root = Some(PathBuf::from(args.next().ok_or("Missing workspace path")?));
        }
    }
    Ok(root)
}
#[tauri::command]
pub fn recovery_restart(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    context: tauri::State<'_, Context>,
) -> Result<(), String> {
    authorize(&window)?;
    super::workspaces::spawn(&context.root)?;
    app.exit(0);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_restart_arguments_preserve_spaces_and_reject_missing_paths() {
        assert_eq!(
            workspace_argument(
                ["--workspace", "/tmp/Logic Project"]
                    .into_iter()
                    .map(Into::into)
            )
            .unwrap(),
            Some(PathBuf::from("/tmp/Logic Project"))
        );
        assert!(workspace_argument(["--workspace"].into_iter().map(Into::into)).is_err());
        assert!(workspace_argument(
            ["--workspace", "a", "--workspace", "b"]
                .into_iter()
                .map(Into::into)
        )
        .is_err());
    }
}
