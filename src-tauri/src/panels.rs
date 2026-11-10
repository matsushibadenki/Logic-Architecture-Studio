use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Mutex};
use tauri::{Emitter, Manager};
pub const IDS: [&str; 3] = ["library", "inspector", "results"];
#[derive(Default)]
pub struct Panels {
    pub homes: Mutex<BTreeMap<String, String>>,
    pub placement: Mutex<()>,
}
pub fn call(app: &tauri::AppHandle, method: &str, args: Value) -> Result<Value, String> {
    studio_service::call(
        &app.state::<super::CoreRoot>().0,
        &studio_service::Request {
            method: method.into(),
            args,
            actor: "gui".into(),
        },
    )
}
pub fn position<'a>(s: &'a Value, id: &str) -> &'a str {
    s["panels"][id].as_str().unwrap_or(match id {
        "library" => "left",
        "results" => "bottom",
        _ => "right",
    })
}
#[tauri::command]
pub async fn panel_place(
    app: tauri::AppHandle,
    panel: String,
    position: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || place(&app, &panel, &position))
        .await
        .map_err(|e| e.to_string())?
}
pub fn place(app: &tauri::AppHandle, id: &str, target: &str) -> Result<(), String> {
    let panels = app.state::<Panels>();
    let _placement = panels.placement.lock().map_err(|e| e.to_string())?;
    if !IDS.contains(&id)
        || !["left", "right", "bottom", "floating", "hidden", "restore"].contains(&target)
    {
        return Err("Invalid panel placement".into());
    }
    let snapshot = call(app, "project.inspect", json!({}))?;
    let previous = position(&snapshot, id);
    let state = app.state::<Panels>();
    let mut homes = state.homes.lock().map_err(|e| e.to_string())?;
    if ["left", "right", "bottom"].contains(&previous) {
        homes.insert(id.into(), previous.into());
    }
    let target = if target == "restore" {
        homes.get(id).map(String::as_str).unwrap_or(match id {
            "library" => "left",
            "results" => "bottom",
            _ => "right",
        })
    } else {
        target
    }
    .to_string();
    let web = app
        .get_webview(&format!("panel-{id}"))
        .ok_or("Panel WebView missing")?;
    if target == "floating" {
        let label = format!("float-{id}");
        let window = match app.get_window(&label) {
            Some(w) => w,
            None => {
                let w = tauri::WindowBuilder::new(app, &label)
                    .title(format!("Logic Studio · {id}"))
                    .inner_size(420., 620.)
                    .min_inner_size(300., 250.)
                    .visible(false)
                    .build()
                    .map_err(|e| e.to_string())?;
                // Mark this as a multi-WebView window before reparenting. On
                // macOS Wry otherwise derives window sizes from the first
                // reparented WebView, which still has its old docked bounds.
                let anchor = w
                    .add_child(
                        tauri::WebviewBuilder::new(
                            format!("anchor-{id}"),
                            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
                        ),
                        tauri::LogicalPosition::new(0., 0.),
                        tauri::LogicalSize::new(1., 1.),
                    )
                    .map_err(|e| e.to_string())?;
                anchor.hide().map_err(|e| e.to_string())?;
                super::layout_store::restore(app, &w, id)?;
                let a = app.clone();
                let panel = id.to_string();
                w.on_window_event(move |e| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        let a = a.clone();
                        let p = panel.clone();
                        std::thread::spawn(move || {
                            let _ = place(&a, &p, "hidden");
                        });
                    } else if matches!(
                        e,
                        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
                    ) {
                        let resized = if let tauri::WindowEvent::Resized(size) = e {
                            Some(*size)
                        } else {
                            None
                        };
                        let a = a.clone();
                        let p = panel.clone();
                        std::thread::spawn(move || {
                            if let Some(size) = resized {
                                if let Some(web) = a.get_webview(&format!("panel-{p}")) {
                                    let _ = web.set_bounds(tauri::Rect {
                                        position: tauri::PhysicalPosition::new(0, 0).into(),
                                        size: size.into(),
                                    });
                                }
                                super::layout_store::capture_size(&a, &p, size);
                            } else {
                                super::layout_store::capture(&a, &p);
                            }
                            if let Err(e) = super::layout_store::save(&a) {
                                eprintln!("Panel frame save: {e}");
                            }
                        });
                    }
                });
                w
            }
        };
        let size = window.inner_size().map_err(|e| e.to_string())?;
        web.reparent(&window).map_err(|e| e.to_string())?;
        web.set_bounds(tauri::Rect {
            position: tauri::PhysicalPosition::new(0, 0).into(),
            size: size.into(),
        })
        .map_err(|e| e.to_string())?;
        web.show().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    } else {
        let home = app.get_window("workspace").ok_or("Workspace missing")?;
        web.reparent(&home).map_err(|e| e.to_string())?;
        if target == "hidden" {
            web.hide().map_err(|e| e.to_string())?;
        }
        super::layout_store::capture(app, id);
        if let Some(w) = app.get_window(&format!("float-{id}")) {
            w.hide().map_err(|e| e.to_string())?;
        }
    }
    call(
        app,
        "view.preferences",
        json!({"panel":id,"position":target}),
    )?;
    if ["left", "right", "bottom"].contains(&target.as_str()) {
        homes.insert(id.into(), target.clone());
    }
    drop(homes);
    super::layout_store::capture(app, id);
    super::layout_store::save(app)?;
    Ok(())
}
pub fn layout(app: &tauri::AppHandle, s: &Value, size: [u32; 2], scale: f64) -> Option<[f32; 4]> {
    let top = (56. * scale) as u32;
    let values = app
        .state::<Gestures>()
        .sizes
        .lock()
        .unwrap()
        .unwrap_or([300., 300., 230.]);
    let lw = ((values[0] * scale) as u32).min(size[0] * 35 / 100);
    let rw = ((values[1] * scale) as u32).min(size[0] * 35 / 100);
    let bottom = ((values[2] * scale) as u32).min(size[1] / 2);
    let has = |p: &str| IDS.iter().any(|id| position(s, id) == p);
    let left = if has("left") { lw } else { 0 };
    let right = if has("right") { rw } else { 0 };
    let bh = if has("bottom") { bottom } else { 0 };
    let graph = [
        left as f32,
        top as f32,
        size[0].saturating_sub(left + right) as f32,
        size[1].saturating_sub(top + bh) as f32,
    ];
    // Reparent holds Wry's window-id lock while awaiting the event loop.
    // Never wait for that lock from a main-thread redraw.
    let panels = app.state::<Panels>();
    let Ok(_placement) = panels.placement.try_lock() else {
        return None;
    };
    if let Some(w) = app.get_webview("controls") {
        let _ = w.set_bounds(tauri::Rect {
            position: tauri::PhysicalPosition::new(0, 0).into(),
            size: tauri::PhysicalSize::new(size[0], top).into(),
        });
    }
    let grip = (7. * scale) as u32;
    for (zone, x, y, w, h, visible) in [
        ("left", left, top, grip, size[1] - top - bh, left > 0),
        (
            "right",
            size[0] - right - grip,
            top,
            grip,
            size[1] - top - bh,
            right > 0,
        ),
        (
            "bottom",
            left,
            size[1] - bh - grip,
            size[0] - left - right,
            grip,
            bh > 0,
        ),
    ] {
        if let Some(web) = app.get_webview(&format!("split-{zone}")) {
            if visible {
                let i = splitter_index(zone).ok()?;
                let info = metrics(i, values, size, scale);
                let state = app.state::<Gestures>();
                let mut announced = state.announced.lock().ok()?;
                if announced.get(zone) != Some(&info)
                    && app
                        .emit_to(
                            tauri::EventTarget::webview(web.label()),
                            "splitter-state",
                            &info,
                        )
                        .is_ok()
                {
                    announced.insert(zone.into(), info);
                }
                drop(announced);
                let _ = web.set_bounds(tauri::Rect {
                    position: tauri::PhysicalPosition::new(x as i32, y as i32).into(),
                    size: tauri::PhysicalSize::new(w, h).into(),
                });
                let _ = web.show();
            } else {
                let _ = web.hide();
            }
        }
    }
    for zone in ["left", "right", "bottom"] {
        let ids: Vec<_> = IDS.iter().filter(|id| position(s, id) == zone).collect();
        let count = ids.len().max(1) as u32;
        for (i, id) in ids.iter().enumerate() {
            let (x, y, w, h) = match zone {
                "left" => (
                    0,
                    top + i as u32 * (size[1] - top - bh) / count,
                    if zone == "left" { left } else { right },
                    (size[1] - top - bh) / count,
                ),
                "right" => (
                    size[0] - right,
                    top + i as u32 * (size[1] - top - bh) / count,
                    if zone == "left" { left } else { right },
                    (size[1] - top - bh) / count,
                ),
                _ => (
                    left + i as u32 * (size[0] - left - right) / count,
                    size[1] - bh,
                    (size[0] - left - right) / count,
                    bh,
                ),
            };
            if let Some(web) = app.get_webview(&format!("panel-{id}")) {
                let _ = web.set_bounds(tauri::Rect {
                    position: tauri::PhysicalPosition::new(x as i32, y as i32).into(),
                    size: tauri::PhysicalSize::new(w.max(1), h.max(1)).into(),
                });
                let _ = web.show();
            }
        }
    }
    for id in IDS {
        if position(s, id) == "hidden" {
            if let Some(w) = app.get_webview(&format!("panel-{id}")) {
                let _ = w.hide();
            }
        }
    }
    Some(graph)
}

#[derive(Clone)]
pub struct Drag {
    kind: String,
    panel: String,
    start: [f64; 2],
    sizes: [f64; 3],
    target: Option<String>,
}
#[derive(Default)]
pub struct Gestures {
    pub sizes: Mutex<Option<[f64; 3]>>,
    active: Mutex<Option<Drag>>,
    announced: Mutex<BTreeMap<String, SplitterMetrics>>,
}
#[derive(Clone, PartialEq, serde::Serialize)]
pub struct SplitterMetrics {
    value: f64,
    min: f64,
    max: f64,
}
fn splitter_index(zone: &str) -> Result<usize, String> {
    match zone {
        "left" => Ok(0),
        "right" => Ok(1),
        "bottom" => Ok(2),
        _ => Err("Invalid splitter".into()),
    }
}
fn metrics(index: usize, values: [f64; 3], size: [u32; 2], scale: f64) -> SplitterMetrics {
    let max = (if index == 2 {
        (size[1] / 2) as f64
    } else {
        (size[0] * 35 / 100) as f64
    } / scale)
        .min(700.);
    let min = (if index == 2 { 120.0_f64 } else { 180.0_f64 }).min(max);
    SplitterMetrics {
        value: values[index].clamp(min, max),
        min,
        max,
    }
}
fn splitter_metrics(app: &tauri::AppHandle, zone: &str) -> Result<SplitterMetrics, String> {
    let i = splitter_index(zone)?;
    let window = app.get_window("workspace").ok_or("Workspace missing")?;
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let values = app
        .state::<Gestures>()
        .sizes
        .lock()
        .map_err(|e| e.to_string())?
        .unwrap_or([300., 300., 230.]);
    Ok(metrics(i, values, [size.width, size.height], scale))
}
fn authorize_splitter(window: &tauri::Webview, zone: &str) -> Result<(), String> {
    splitter_index(zone)?;
    if window.label() != format!("split-{zone}") {
        return Err("Splitter window required".into());
    }
    Ok(())
}
#[tauri::command]
pub fn panel_splitter_focus(app: tauri::AppHandle, zone: String) -> Result<(), String> {
    splitter_index(&zone)?;
    let docked = app
        .state::<super::sync::Cache>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|snapshot| IDS.iter().any(|id| position(&snapshot.meta, id) == zone));
    if !docked {
        return Ok(());
    }
    let web = app
        .get_webview(&format!("split-{zone}"))
        .ok_or("Splitter missing")?;
    web.set_focus().map_err(|e| e.to_string())?;
    app.emit_to(
        tauri::EventTarget::webview(web.label()),
        "splitter-focus",
        (),
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn panel_splitter_state(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    zone: String,
) -> Result<SplitterMetrics, String> {
    authorize_splitter(&webview, &zone)?;
    splitter_metrics(&app, &zone)
}
fn key_size(zone: &str, key: &str, coarse: bool, current: &SplitterMetrics) -> Result<f64, String> {
    let direction = match (zone, key) {
        (_, "Home") => return Ok(current.min),
        (_, "End") => return Ok(current.max),
        ("left", "ArrowLeft") | ("right", "ArrowRight") | ("bottom", "ArrowDown") => -1.,
        ("left", "ArrowRight") | ("right", "ArrowLeft") | ("bottom", "ArrowUp") => 1.,
        _ => return Err("Invalid splitter key".into()),
    };
    Ok(
        (current.value + direction * if coarse { 50. } else { 10. })
            .clamp(current.min, current.max),
    )
}
#[tauri::command]
pub async fn panel_splitter_key(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    zone: String,
    key: String,
    coarse: bool,
) -> Result<SplitterMetrics, String> {
    authorize_splitter(&webview, &zone)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Gestures>();
        let active = state.active.lock().map_err(|e| e.to_string())?;
        if active.is_some() {
            return Err("Another layout gesture is active".into());
        }
        let i = splitter_index(&zone)?;
        let current = splitter_metrics(&app, &zone)?;
        let value = key_size(&zone, &key, coarse, &current)?;
        let before = *state.sizes.lock().map_err(|e| e.to_string())?;
        let mut sizes = before.unwrap_or([300., 300., 230.]);
        // Saved dimensions retain their normal minimum even in a tiny window.
        sizes[i] = value.max(if i == 2 { 120. } else { 180. });
        *state.sizes.lock().map_err(|e| e.to_string())? = Some(sizes);
        if let Err(error) = super::layout_store::save(&app) {
            *state.sizes.lock().map_err(|e| e.to_string())? = before;
            return Err(error);
        }
        drop(active);
        splitter_metrics(&app, &zone)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn panel_gesture(
    app: tauri::AppHandle,
    kind: String,
    panel: String,
    phase: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || gesture(&app, &kind, &panel, &phase, [x, y]))
        .await
        .map_err(|e| e.to_string())?
}
fn gesture(
    app: &tauri::AppHandle,
    kind: &str,
    panel: &str,
    phase: &str,
    p: [f64; 2],
) -> Result<(), String> {
    if !["start", "move", "end", "cancel"].contains(&phase) {
        return Err("Invalid gesture phase".into());
    }
    if !p.iter().all(|v| v.is_finite()) {
        return Err("Invalid pointer".into());
    }
    let state = app.state::<Gestures>();
    let mut active = state.active.lock().map_err(|e| e.to_string())?;
    if phase == "start" {
        if active.is_some() {
            return Err("Another layout gesture is active".into());
        }
        if kind != "dock" && !["left", "right", "bottom"].contains(&kind) {
            return Err("Invalid splitter".into());
        }
        if kind == "dock" && !IDS.contains(&panel) {
            return Err("Invalid panel".into());
        }
        let sizes = state.sizes.lock().unwrap().unwrap_or([300., 300., 230.]);
        *active = Some(Drag {
            kind: kind.into(),
            panel: panel.into(),
            start: p,
            sizes,
            target: None,
        });
        return Ok(());
    }
    let Some(d) = active.as_mut() else {
        return Ok(());
    };
    if d.kind != kind || d.panel != panel {
        return Err("Layout gesture owner mismatch".into());
    }
    if phase == "cancel" {
        *state.sizes.lock().unwrap() = Some(d.sizes);
        *active = None;
        app.state::<super::native::Shared>()
            .lock()
            .unwrap()
            .renderer
            .dock_target = None;
        return Ok(());
    }
    if d.kind == "dock" {
        let w = app.get_window("workspace").ok_or("Workspace missing")?;
        let origin = w.inner_position().map_err(|e| e.to_string())?;
        let scale = w.scale_factor().map_err(|e| e.to_string())?;
        let sz = w.inner_size().map_err(|e| e.to_string())?;
        let local = [
            p[0] * scale - origin.x as f64,
            p[1] * scale - origin.y as f64,
        ];
        d.target = if local[0] < 0.
            || local[1] < 0.
            || local[0] > sz.width as f64
            || local[1] > sz.height as f64
        {
            None
        } else if local[1] > sz.height as f64 * 0.65 {
            Some("bottom".into())
        } else if local[0] < sz.width as f64 * 0.35 {
            Some("left".into())
        } else if local[0] > sz.width as f64 * 0.65 {
            Some("right".into())
        } else {
            None
        };
        let shared = app.state::<super::native::Shared>();
        let mut n = shared.lock().unwrap();
        let [gx, gy, gw, gh] = n.graph_rect;
        n.renderer.dock_target = d.target.as_deref().map(|t| match t {
            "left" => [gx, gy, gw * 0.3, gh],
            "right" => [gx + gw * 0.7, gy, gw * 0.3, gh],
            _ => [gx, gy + gh * 0.7, gw, gh * 0.3],
        });
    } else {
        let mut sizes = d.sizes;
        let i = match d.kind.as_str() {
            "left" => 0,
            "right" => 1,
            _ => 2,
        };
        let delta = if i == 2 {
            d.start[1] - p[1]
        } else if i == 1 {
            d.start[0] - p[0]
        } else {
            p[0] - d.start[0]
        };
        sizes[i] = (sizes[i] + delta).clamp(if i == 2 { 120. } else { 180. }, 700.);
        *state.sizes.lock().unwrap() = Some(sizes);
    }
    if phase == "end" {
        let d = active.take().unwrap();
        app.state::<super::native::Shared>()
            .lock()
            .unwrap()
            .renderer
            .dock_target = None;
        drop(active);
        if d.kind == "dock" {
            if let Some(t) = d.target {
                place(app, &d.panel, &t)?;
            }
        } else {
            super::layout_store::save(app)?;
        }
    }
    Ok(())
}
