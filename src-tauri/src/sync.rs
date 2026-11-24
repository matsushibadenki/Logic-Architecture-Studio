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
    let cache = app.state::<Cache>();
    let mut current = cache.0.lock().unwrap();
    if current.as_ref().map(|s| &s.cursor) != previous.as_ref().map(|s| &s.cursor) {
        // A commit acknowledgement already advanced this cache.
        return Ok(());
    }
    let theme_changed = previous
        .as_ref()
        .is_none_or(|p| p.meta["theme"] != snapshot.meta["theme"]);
    let light = snapshot.meta["theme"] == "light";
    let revision = snapshot.meta["revision"].as_u64().unwrap_or(0);
    *current = Some(snapshot);
    drop(current);
    if theme_changed {
        app.set_theme(Some(if light {
            tauri::Theme::Light
        } else {
            tauri::Theme::Dark
        }));
    }
    super::measurement::native_transfer(app, revision, &response);
    Ok(())
}

pub fn accept_commit(app: &tauri::AppHandle, response: &serde_json::Value) -> bool {
    let Some(base) = response["base"].as_str() else {
        return false;
    };
    let cache = app.state::<Cache>();
    let mut current = cache.0.lock().unwrap();
    if current
        .as_ref()
        .is_none_or(|snapshot| snapshot.cursor != base)
    {
        return false;
    }
    if let Ok(snapshot) = studio_service::native_sync::apply(current.as_ref(), response) {
        let revision = snapshot.meta["revision"].as_u64().unwrap_or(0);
        *current = Some(snapshot);
        drop(current);
        super::measurement::native_transfer(app, revision, response);
        return true;
    }
    false
}
