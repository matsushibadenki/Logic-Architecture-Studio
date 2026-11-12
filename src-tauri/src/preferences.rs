use tauri::Manager;
#[tauri::command]
pub async fn preferences_open(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("preferences") {
        window.unminimize().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "preferences",
        tauri::WebviewUrl::App("index.html?preferences=1".into()),
    )
    .title("Preferences")
    .inner_size(780., 680.)
    .min_inner_size(620., 480.)
    .center()
    .build()
    .map_err(|e| e.to_string())?
    .set_focus()
    .map_err(|e| e.to_string())
}
