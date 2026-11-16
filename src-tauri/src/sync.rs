use serde_json::json;
use std::sync::Mutex;
use tauri::Manager;

pub use studio_service::native_sync::Snapshot;
#[derive(Default)]
pub struct Cache(pub Mutex<Option<Snapshot>>);
pub struct Wake(pub std::sync::mpsc::SyncSender<()>);
pub fn wake(app: &tauri::AppHandle) {
    let _ = app.state::<Wake>().0.try_send(());
}

/// Socket I/O and document decoding stay off the UI/event-loop thread.
pub fn poll(app: &tauri::AppHandle) -> Result<(), String> {
    let previous = app.state::<Cache>().0.lock().unwrap().clone();
    let mut response = studio_service::call(
        &app.state::<super::CoreRoot>().0,
        &studio_service::Request {
            method: "project.sync".into(),
            actor: "native".into(),
            args: json!({"scope":"native_delta", "cursor":previous.as_ref().map(|s| &s.cursor),
                "document_revision":previous.as_ref().map(|s| s.meta["revision"].clone())}),
        },
    )?;
    if response["changed"] != true {
        return Ok(());
    }
    let snapshot = match studio_service::native_sync::apply(previous.as_ref(), &response) {
        Ok(snapshot) => snapshot,
        Err(_) => {
            response = studio_service::call(
                &app.state::<super::CoreRoot>().0,
                &studio_service::Request {
                    method: "project.sync".into(),
                    actor: "native".into(),
                    args: json!({"scope":"native_delta"}),
                },
            )?;
            studio_service::native_sync::apply(None, &response)?
        }
    };
    if previous
        .as_ref()
        .is_none_or(|p| p.meta["theme"] != snapshot.meta["theme"])
    {
        app.set_theme(Some(if snapshot.meta["theme"] == "light" {
            tauri::Theme::Light
        } else {
            tauri::Theme::Dark
        }));
    }
    super::measurement::native_transfer(
        app,
        snapshot.meta["revision"].as_u64().unwrap_or(0),
        &response,
    );
    *app.state::<Cache>().0.lock().unwrap() = Some(snapshot);
    Ok(())
}
