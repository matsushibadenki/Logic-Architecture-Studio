#[tauri::command]
pub async fn save_testbench_csv(
    webview: tauri::Webview,
    root: tauri::State<'_, super::CoreRoot>,
    expected_revision: u64,
    expected_version: u64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path = root.0.clone();
        let result = tauri::async_runtime::spawn_blocking(move || studio_service::call(&path, &studio_service::Request {
            method: "testbench.export_csv".into(), args: serde_json::json!({"expected_revision":expected_revision,"expected_version":expected_version}), actor: "gui".into(),
        })).await.map_err(|e| e.to_string())??;
        if let Some(file) = rfd::AsyncFileDialog::new()
            .set_parent(&webview.window())
            .set_file_name("stimuli.csv")
            .add_filter("CSV", &["csv"])
            .save_file()
            .await
        {
            let source = result["source"]
                .as_str()
                .ok_or("Missing CSV source")?
                .to_string();
            tauri::async_runtime::spawn_blocking(move || std::fs::write(file.path(), source))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (webview, root, expected_revision, expected_version);
        Err("Native CSV export is not supported on this platform yet".into())
    }
}
