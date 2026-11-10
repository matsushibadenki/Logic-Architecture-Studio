#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod layout_store;
mod measurement;
mod native;
mod panels;
mod recovery;
mod sync;
mod workspaces;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager};
struct CoreRoot(PathBuf);
#[tauri::command]
async fn command(
    app: tauri::AppHandle,
    root: tauri::State<'_, CoreRoot>,
    method: String,
    mut args: Value,
) -> Result<Value, String> {
    let notify = !matches!(
        method.as_str(),
        "project.sync" | "project.inspect" | "design.get_ir"
    );
    if method == "project.sync"
        && args["scope"] == "web"
        && app.state::<measurement::Trace>().full_sync
    {
        args["scope"] = json!("full");
    }
    let path = root.0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        studio_service::call(
            &path,
            &studio_service::Request {
                method,
                args,
                actor: "gui".into(),
            },
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    if notify {
        sync::wake(&app);
        let _ = app.emit("design-changed", ());
    }
    Ok(result)
}
#[tauri::command]
async fn float_panel(app: tauri::AppHandle, panel: String) -> Result<(), String> {
    panels::panel_place(app, panel, "floating".into()).await
}
#[tauri::command]
async fn dock_panel(app: tauri::AppHandle, panel: String) -> Result<(), String> {
    panels::panel_place(app, panel, "restore".into()).await
}
#[tauri::command]
async fn gpu_view(app: tauri::AppHandle, root: tauri::State<'_, CoreRoot>) -> Result<(), String> {
    if let Some(w) = app.get_window("gpu-circuit") {
        return w.set_focus().map_err(|e| e.to_string());
    }
    let window = Arc::new(
        tauri::WindowBuilder::new(&app, "gpu-circuit")
            .title("Circuit geometry · Rust / wgpu")
            .inner_size(1100., 720.)
            .build()
            .map_err(|e| e.to_string())?,
    );
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let renderer = Arc::new(Mutex::new(studio_renderer::Renderer::new(
        window.clone(),
        size.width,
        size.height,
    )?));
    let r = renderer.clone();
    let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let a = alive.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(size) => {
            if let Ok(mut r) = r.lock() {
                r.resize(size.width, size.height);
            }
        }
        tauri::WindowEvent::Destroyed => a.store(false, std::sync::atomic::Ordering::Relaxed),
        _ => {}
    });
    let path = root.0.clone();
    std::thread::spawn(move || {
        let _window = window;
        while alive.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(snapshot) = studio_service::call(
                &path,
                &studio_service::Request {
                    method: "design.get_ir".into(),
                    args: json!({}),
                    actor: "renderer".into(),
                },
            ) {
                if let Ok(doc) =
                    serde_json::from_value::<design_core::Document>(snapshot["document"].clone())
                {
                    if let Ok(mut r) = renderer.lock() {
                        if let Err(e) = r.draw(
                            &doc,
                            snapshot["selected"].as_str(),
                            snapshot["locale"].as_str().unwrap_or("en"),
                        ) {
                            eprintln!("Renderer: {e}");
                            break;
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    Ok(())
}
fn main() {
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .setup(|app| {
            let selection = workspaces::Store::new(
                std::env::var_os("LAS_SETTINGS_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or(app.path().app_config_dir()?),
            );
            let remembered = if std::env::var_os("LAS_WORKSPACE").is_some()
                || std::env::args_os().any(|arg| arg == "--workspace")
            {
                None
            } else {
                match selection.load() {
                    Ok(root) => root,
                    Err(error) => {
                        app.manage(workspaces::State::new(selection, None, Some(error)));
                        workspaces::show_start(app.handle())?;
                        return Ok(());
                    }
                }
            };
            let root = if let Some(path) = recovery::workspace_argument(std::env::args_os().skip(1))
                .map_err(std::io::Error::other)?
            {
                path
            } else if let Some(path) = std::env::var_os("LAS_WORKSPACE") {
                PathBuf::from(path)
            } else if let Some(path) = remembered {
                path
            } else if cfg!(debug_assertions) {
                let cwd = std::env::current_dir()?;
                if cwd.file_name().is_some_and(|s| s == "src-tauri") {
                    cwd.parent().unwrap().into()
                } else if cwd.join("Cargo.toml").exists() {
                    cwd
                } else {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .parent()
                        .unwrap()
                        .to_path_buf()
                }
            } else {
                app.path().app_data_dir()?.join("workspace")
            };
            std::fs::create_dir_all(&root)?;
            let root = root.canonicalize()?;
            app.manage(workspaces::State::new(selection, Some(root.clone()), None));
            let store = match layout_store::Store::load(&root) {
                Ok(store) => store,
                Err(error) => {
                    recovery::show(app.handle(), root, error, true)?;
                    return Ok(());
                }
            };
            if studio_service::call(
                &root,
                &studio_service::Request {
                    method: "project.inspect".into(),
                    args: json!({}),
                    actor: "gui".into(),
                },
            )
            .is_err()
            {
                let service = match studio_service::Service::open(&root) {
                    Ok(service) => service,
                    Err(error) => {
                        recovery::show(app.handle(), root, error, false)?;
                        return Ok(());
                    }
                };
                std::thread::spawn(move || {
                    if let Err(e) = service.listen() {
                        eprintln!("Core: {e}");
                    }
                });
            }
            let saved = store.data.lock().unwrap().clone();
            app.manage(CoreRoot(root));
            let panels = panels::Panels::default();
            *panels.homes.lock().unwrap() = saved.homes;
            let gestures = panels::Gestures::default();
            *gestures.sizes.lock().unwrap() = Some(saved.sizes);
            app.manage(panels);
            app.manage(gestures);
            app.manage(store);
            app.manage(sync::Cache::default());
            app.manage(measurement::Trace::default());
            let (wake, receiver) = std::sync::mpsc::sync_channel(1);
            app.manage(sync::Wake(wake));
            let window = tauri::WindowBuilder::new(app, "workspace")
                .title("Logic Architecture Studio")
                .inner_size(1440., 900.)
                .min_inner_size(800., 540.)
                .build()?;
            let size = window.inner_size()?;
            let renderer =
                studio_renderer::Renderer::new(Arc::new(window.clone()), size.width, size.height)
                    .map_err(std::io::Error::other)?;
            app.manage(Arc::new(Mutex::new(native::Native {
                renderer,
                dirty: true,
                source: None,
                frame: None,
                selection_cursor: String::new(),
                pending_revision: None,
                view: Default::default(),
                position: None,
                down: false,
                graph_rect: [0., 56., 1000., 700.],
            })));
            window.add_child(
                tauri::webview::WebviewBuilder::new(
                    "controls",
                    tauri::WebviewUrl::App("index.html?native=1".into()),
                ),
                tauri::LogicalPosition::new(1060., 0.),
                tauri::LogicalSize::new(380., 900.),
            )?;
            for id in panels::IDS {
                window.add_child(
                    tauri::webview::WebviewBuilder::new(
                        format!("panel-{id}"),
                        tauri::WebviewUrl::App(format!("index.html?panel={id}&docked=1").into()),
                    ),
                    tauri::LogicalPosition::new(0., 56.),
                    tauri::LogicalSize::new(300., 300.),
                )?;
            }
            let restore = app.handle().clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    if let Ok(s) = panels::call(&restore, "project.inspect", serde_json::json!({}))
                    {
                        for id in panels::IDS {
                            let p = panels::position(&s, id);
                            if p == "floating" || p == "hidden" {
                                if let Err(e) = panels::place(&restore, id, p) {
                                    eprintln!("Panel restore: {e}");
                                }
                            }
                        }
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            });
            for zone in ["left", "right", "bottom"] {
                window.add_child(
                    tauri::webview::WebviewBuilder::new(
                        format!("split-{zone}"),
                        tauri::WebviewUrl::App(format!("index.html?split={zone}").into()),
                    ),
                    tauri::LogicalPosition::new(0., 56.),
                    tauri::LogicalSize::new(7., 7.),
                )?;
            }
            app.wry_plugin(native::Bridge(app.handle().clone()));
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                let _ = receiver.recv_timeout(Duration::from_millis(100));
                if let Err(e) = sync::poll(&handle) {
                    eprintln!("Native sync: {e}");
                }
                let h = handle.clone();
                if handle
                    .run_on_main_thread(move || native::redraw(&h))
                    .is_err()
                {
                    break;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            command,
            workspaces::workspace_info,
            workspaces::workspace_pick,
            recovery::recovery_status,
            recovery::recovery_choose_backup,
            recovery::recovery_apply,
            recovery::recovery_restart,
            measurement::measurement_enabled,
            measurement::web_frame_ack,
            float_panel,
            dock_panel,
            gpu_view,
            panels::panel_place,
            panels::panel_gesture,
            panels::panel_splitter_state,
            panels::panel_splitter_focus,
            panels::panel_splitter_key
        ])
        .build(context)
        .expect("Cannot run Logic Architecture Studio")
        .run(|_, _| {});
}
