use serde_json::json;
use std::sync::{Arc, Mutex};
use studio_renderer::{
    editor::EditorView,
    unge_interaction::{PointerButton, PointerEvent},
};
use tauri::{Emitter, EventLoopMessage, Manager};
use tauri_runtime_wry::tao::{
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoopProxy, EventLoopWindowTarget},
    keyboard::ModifiersState,
};
use tauri_runtime_wry::{
    Context, EventLoopIterationContext, Message, Plugin, PluginBuilder, WebContextStore,
};
type FrameStamp = (String, [u32; 2], f64, [f64; 3], Option<[f32; 4]>);
pub struct Native {
    pub renderer: studio_renderer::Renderer,
    pub view: EditorView,
    pub position: Option<[f32; 2]>,
    pub down: bool,
    pub primary_press: Option<[f32; 2]>,
    pub last_module_click: Option<(String, [f32; 2], ClickTime)>,
    pub modifiers: ModifiersState,
    pub graph_rect: [f32; 4],
    pub dirty: bool,
    pub source: Option<Arc<design_core::Document>>,
    pub frame: Option<FrameStamp>,
    pub selection_cursor: String,
    pub pending_revision: Option<(String, u64)>,
}
pub struct ClickTime {
    received: std::time::Instant,
    appkit: Option<f64>,
    system_double_click: bool,
}

impl ClickTime {
    fn follows(&self, previous: &Self, interval: f64) -> bool {
        if self.system_double_click {
            return true;
        }
        match (self.appkit, previous.appkit) {
            (Some(now), Some(before)) if now > before => now - before <= interval,
            _ => {
                self.received
                    .duration_since(previous.received)
                    .as_secs_f64()
                    <= interval
            }
        }
    }
}

#[cfg(target_os = "macos")]
struct AppKitClick {
    position: [f32; 2],
    timestamp: f64,
    double_click: bool,
    modifiers: ModifiersState,
}

// Tao queues mouse motion and clicks separately. Use the AppKit event when
// available so a delayed redraw or missing motion event cannot skew the click.
#[cfg(target_os = "macos")]
fn appkit_click(window: &tauri::Window, state: ElementState) -> Option<AppKitClick> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSEventType};
    let mtm = MainThreadMarker::new()?;
    let event = NSApplication::sharedApplication(mtm).currentEvent()?;
    let expected = match state {
        ElementState::Pressed => NSEventType::LeftMouseDown,
        ElementState::Released => NSEventType::LeftMouseUp,
        _ => return None,
    };
    if event.r#type() != expected {
        return None;
    }
    let event_window = event.window(mtm)?;
    if std::ptr::from_ref(&*event_window).cast::<std::ffi::c_void>()
        != window.ns_window().ok()?.cast_const()
    {
        return None;
    }
    let view = event_window.contentView()?;
    let point = view.convertPoint_fromView(event.locationInWindow(), None);
    let scale = event_window.backingScaleFactor();
    let flags = event.modifierFlags();
    let mut modifiers = ModifiersState::empty();
    modifiers.set(
        ModifiersState::SHIFT,
        flags.contains(NSEventModifierFlags::Shift),
    );
    modifiers.set(
        ModifiersState::CONTROL,
        flags.contains(NSEventModifierFlags::Control),
    );
    modifiers.set(
        ModifiersState::ALT,
        flags.contains(NSEventModifierFlags::Option),
    );
    modifiers.set(
        ModifiersState::SUPER,
        flags.contains(NSEventModifierFlags::Command),
    );
    Some(AppKitClick {
        position: [
            (point.x * scale) as f32,
            ((view.bounds().size.height - point.y) * scale) as f32,
        ],
        timestamp: event.timestamp(),
        double_click: event.clickCount() == 2,
        modifiers,
    })
}

fn double_click_interval() -> f64 {
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSEvent::doubleClickInterval()
    }
    #[cfg(not(target_os = "macos"))]
    {
        0.5
    }
}

fn in_canvas(point: [f32; 2], rect: [f32; 4]) -> bool {
    point[0] >= rect[0]
        && point[1] >= rect[1]
        && point[0] < rect[0] + rect[2]
        && point[1] < rect[1] + rect[3]
}

pub type Shared = Arc<Mutex<Native>>;
#[tauri::command]
pub async fn open_module(app: tauri::AppHandle, definition: String) -> Result<(), String> {
    let exists = app
        .state::<super::sync::Cache>()
        .0
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|snapshot| snapshot.document.modules.contains_key(&definition));
    if !exists {
        return Err("Unknown module definition".into());
    }
    show_module_window(&app, definition).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn return_to_workspace(window: tauri::WebviewWindow) -> Result<(), String> {
    if !window.label().starts_with("module-") {
        return Err("Expected a module editor window".into());
    }
    let workspace = window
        .app_handle()
        .get_window("workspace")
        .ok_or("Workspace is closed")?;
    window.close().map_err(|e| e.to_string())?;
    workspace.set_focus().map_err(|e| e.to_string())
}

fn show_module_window(app: &tauri::AppHandle, definition: String) -> tauri::Result<()> {
    let label = format!("module-{definition}");
    if let Some(window) = app.get_webview_window(&label) {
        window.unminimize()?;
        return window.set_focus();
    }
    tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App(format!("index.html?module={definition}").into()),
    )
    .title(definition)
    .inner_size(1180., 760.)
    .min_inner_size(740., 460.)
    .build()?;
    Ok(())
}
pub fn redraw(app: &tauri::AppHandle) {
    let Some(w) = app.get_window("workspace") else {
        return;
    };
    let Ok(size) = w.inner_size() else { return };
    if size.width == 0 || size.height == 0 {
        return;
    }
    let scale = w.scale_factor().unwrap_or(1.);
    let Some(snapshot) = app.state::<super::sync::Cache>().0.lock().unwrap().clone() else {
        return;
    };
    let s = &snapshot.meta;
    let doc = &snapshot.document;
    let sizes = app
        .state::<super::panels::Gestures>()
        .sizes
        .lock()
        .unwrap()
        .unwrap_or([300., 300., 230.]);
    let shared = app.state::<Shared>();
    let mut n = shared.lock().unwrap();
    if let Some((epoch, revision)) = &n.pending_revision {
        let current_epoch = snapshot
            .cursor
            .rsplit_once(':')
            .map(|(e, _)| e)
            .unwrap_or("");
        if current_epoch == epoch && s["revision"].as_u64().unwrap_or(0) < *revision {
            // Keep the already-presented drag preview until the acknowledged
            // canonical revision arrives. Do not present a stale frame first.
            return;
        }
        n.pending_revision = None;
    }
    let frame = (
        snapshot.cursor.clone(),
        [size.width, size.height],
        scale,
        sizes,
        n.renderer.dock_target,
    );
    if !n.dirty && n.frame.as_ref() == Some(&frame) {
        return;
    }
    let Some(rect) = super::panels::layout(app, s, [size.width, size.height], scale) else {
        // Reparenting is in flight; retry without marking this frame complete.
        return;
    };
    if n.view.ids.is_empty() {
        let min_x = doc.nodes.iter().map(|node| node.x).fold(0., f32::min) - 30.;
        let min_y = doc.nodes.iter().map(|node| node.y).fold(0., f32::min) - 30.;
        let max_x = doc
            .nodes
            .iter()
            .map(|node| node.x + design_core::NODE_WIDTH)
            .fold(0., f32::max)
            + 30.;
        let max_y = doc
            .nodes
            .iter()
            .map(|node| node.y + design_core::node_height(&node.kind))
            .fold(0., f32::max)
            + 30.;
        n.view.viewport.zoom = (rect[2] / (max_x - min_x))
            .min(rect[3] / (max_y - min_y))
            .clamp(0.02, 1.);
        n.view.viewport.origin = [
            min_x - rect[0] / n.view.viewport.zoom,
            min_y - rect[1] / n.view.viewport.zoom,
        ];
    } else if n.graph_rect != rect {
        // Keep the visible graph anchored when dock strips change size.
        n.view.viewport.origin[0] -= (rect[0] - n.graph_rect[0]) / n.view.viewport.zoom;
        n.view.viewport.origin[1] -= (rect[1] - n.graph_rect[1]) / n.view.viewport.zoom;
    }
    if !n.view.interaction.is_active() {
        if n.source.as_ref().is_none_or(|old| !Arc::ptr_eq(old, doc))
            && n.view.sync_document(doc).is_ok()
        {
            n.source = Some(doc.clone());
        }
        if n.selection_cursor != snapshot.cursor {
            n.view.selection = s["selection"]
                .as_array()
                .map(|ids| ids.iter().filter_map(|id| id.as_str()).collect::<Vec<_>>())
                .unwrap_or_else(|| s["selected"].as_str().into_iter().collect())
                .into_iter()
                .filter_map(|id| n.view.ids.get(id).copied())
                .collect();
            n.selection_cursor = snapshot.cursor.clone();
        }
    }
    if n.frame
        .as_ref()
        .is_none_or(|previous| previous.0 != snapshot.cursor)
    {
        n.renderer
            .set_theme(serde_json::from_value(s["theme"].clone()).unwrap_or_default());
        n.renderer
            .set_hide_gate_frames(s["hide_gate_frames"].as_bool().unwrap_or(false));
        n.renderer
            .set_gate_symbols(s["gate_symbols"].as_bool().unwrap_or(false));
        n.renderer
            .set_node_colors(serde_json::from_value(s["node_colors"].clone()).unwrap_or_default());
    }
    n.dirty = false;
    n.frame = Some(frame);
    n.renderer.resize(size.width, size.height);
    n.graph_rect = rect;
    n.view.viewport.size = [size.width as f32, size.height as f32];
    let Native { renderer, view, .. } = &mut *n;
    #[cfg(target_os = "macos")]
    let observe = super::drag_measurement::gpu_observer();
    #[cfg(not(target_os = "macos"))]
    let observe = None;
    let result = renderer.draw_editor(
        doc,
        s["locale"].as_str().unwrap_or("ja"),
        view,
        [size.width, size.height],
        s["revision"].as_u64().unwrap_or(0),
        observe,
    );
    #[cfg(target_os = "macos")]
    if let Ok(Some((assembly_ms, surface))) = &result {
        super::drag_measurement::cpu_sample(*assembly_ms, surface);
    }
    if result.is_ok() {
        super::measurement::native(app, s["revision"].as_u64().unwrap_or(0));
    }
    if let Err(e) = result {
        n.dirty = true;
        eprintln!("Native render: {e}");
    }
}
pub struct Bridge(pub tauri::AppHandle);
pub struct Input {
    app: tauri::AppHandle,
    drag_frame_pending: bool,
}
impl PluginBuilder<EventLoopMessage> for Bridge {
    type Plugin = Input;
    fn build(self, _: Context<EventLoopMessage>) -> Input {
        Input {
            app: self.0,
            drag_frame_pending: false,
        }
    }
}
impl Plugin<EventLoopMessage> for Input {
    fn on_event(
        &mut self,
        event: &Event<Message<EventLoopMessage>>,
        _: &EventLoopWindowTarget<Message<EventLoopMessage>>,
        _: &EventLoopProxy<Message<EventLoopMessage>>,
        _: &mut ControlFlow,
        context: EventLoopIterationContext<'_, EventLoopMessage>,
        _: &WebContextStore,
    ) -> bool {
        if matches!(event, Event::MainEventsCleared) {
            if std::mem::take(&mut self.drag_frame_pending) {
                redraw(&self.app);
            }
            return false;
        }
        if let Event::RedrawRequested(id) = event {
            let ours = context.window_id_map.get(id).is_some_and(|id| {
                context
                    .windows
                    .0
                    .borrow()
                    .get(&id)
                    .is_some_and(|w| w.label() == "workspace")
            });
            if ours {
                self.drag_frame_pending = false;
                self.app.state::<Shared>().lock().unwrap().dirty = true;
                redraw(&self.app);
            }
            return false;
        }
        let Event::WindowEvent {
            window_id, event, ..
        } = event
        else {
            return false;
        };
        let ours = context.window_id_map.get(window_id).is_some_and(|id| {
            context
                .windows
                .0
                .borrow()
                .get(&id)
                .is_some_and(|w| w.label() == "workspace")
        });
        if !ours {
            return false;
        }
        let Some(w) = self.app.get_window("workspace") else {
            return false;
        };
        let scale = w.scale_factor().unwrap_or(1.);
        let root = self.app.state::<super::CoreRoot>();
        let shared = self.app.state::<Shared>();
        let mut n = shared.lock().unwrap();
        let drag_motion = n.down && matches!(event, WindowEvent::CursorMoved { .. });
        n.dirty |= !matches!(event, WindowEvent::CursorMoved { .. }) || n.down;
        let mut input = None;
        let mut primary_click = None;
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                n.modifiers = *modifiers;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = [position.x as f32, position.y as f32];
                if n.down {
                    // The press owns the gesture until release. Crossing a dock
                    // boundary must not discard the preview or its final position.
                    n.position = Some(p);
                    input = Some(PointerEvent::Move {
                        pointer: 0,
                        position: p,
                    });
                } else {
                    n.position = in_canvas(p, n.graph_rect).then_some(p);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let time = ClickTime {
                    received: std::time::Instant::now(),
                    appkit: None,
                    system_double_click: false,
                };
                let position = n.position;
                #[cfg(target_os = "macos")]
                let (position, time) = if let Some(click) = (*button == MouseButton::Left)
                    .then(|| appkit_click(&w, *state))
                    .flatten()
                {
                    n.modifiers = click.modifiers;
                    (
                        Some(click.position),
                        ClickTime {
                            appkit: Some(click.timestamp),
                            system_double_click: click.double_click,
                            ..time
                        },
                    )
                } else {
                    (position, time)
                };
                if let Some(p) = position.filter(|point| {
                    in_canvas(*point, n.graph_rect) || (n.down && *state == ElementState::Released)
                }) {
                    n.position = Some(p);
                    if *state == ElementState::Pressed {
                        let snap = n.modifiers.alt_key();
                        n.view.interaction.set_grid_snap(snap);
                        n.down = true;
                        // Track the click independently of additive selection.
                        n.primary_press = (*button == MouseButton::Left).then_some(p);
                        input = Some(PointerEvent::Down {
                            pointer: 0,
                            position: p,
                            button: if *button == MouseButton::Left {
                                PointerButton::Primary
                            } else {
                                PointerButton::Pan
                            },
                            additive: n.modifiers.shift_key()
                                || n.modifiers.super_key()
                                || n.modifiers.control_key(),
                        });
                    } else if n.down {
                        n.down = false;
                        if *button == MouseButton::Left
                            && n.primary_press.take().is_some_and(|start| {
                                (start[0] - p[0]).hypot(start[1] - p[1]) <= 5. * scale as f32
                            })
                        {
                            primary_click = Some((p, time));
                        } else {
                            n.last_module_click = None;
                        }
                        input = Some(PointerEvent::Up {
                            pointer: 0,
                            position: p,
                        });
                    }
                } else if n.down {
                    n.down = false;
                    n.primary_press = None;
                    n.last_module_click = None;
                    input = Some(PointerEvent::Cancel);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(p) = n.position {
                    let dy = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y * 40.,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / scale as f32,
                        _ => 0.,
                    };
                    n.view
                        .viewport
                        .zoom_at(p, (dy.clamp(-400., 400.) * 0.002).exp());
                }
            }
            WindowEvent::Focused(false) => {
                n.position = None;
                n.primary_press = None;
                n.last_module_click = None;
                n.modifiers = ModifiersState::empty();
                n.down = false;
                input = Some(PointerEvent::Cancel);
            }
            WindowEvent::CursorLeft { .. } if !n.down => {
                n.position = None;
                n.last_module_click = None;
            }
            _ => {}
        }
        if let Some(event) = input {
            if let Some(snapshot) = self
                .app
                .state::<super::sync::Cache>()
                .0
                .lock()
                .unwrap()
                .clone()
            {
                let s = &snapshot.meta;
                let doc = &snapshot.document;
                let rev = s["revision"].as_u64().unwrap();
                let started = std::time::Instant::now();
                match n.view.pointer(doc, rev, event) {
                    Ok(ops) => {
                        let moving = ops.iter().any(|op| {
                            matches!(op, design_core::Operation::UpdateNode { x: Some(_), .. })
                        });
                        if let Some((point, time)) = primary_click.filter(|_| ops.is_empty()) {
                            let target = n.view.node_at(point).and_then(|id| {
                                let node = doc.node(id).ok()?;
                                match &node.config {
                                    design_core::NodeConfig::Module { definition, .. } => {
                                        Some((id.to_owned(), definition.clone()))
                                    }
                                    _ => None,
                                }
                            });
                            let previous = n.last_module_click.take();
                            if let Some((id, definition)) = target {
                                if previous.is_some_and(|(old_id, old_point, old_time)| {
                                    old_id == id
                                        && time.follows(&old_time, double_click_interval())
                                        && (old_point[0] - point[0]).hypot(old_point[1] - point[1])
                                            <= 5. * scale as f32
                                }) {
                                    let app = self.app.clone();
                                    tauri::async_runtime::spawn(async move {
                                        if let Err(error) = open_module(app, definition).await {
                                            eprintln!("Module editor: {error}");
                                        }
                                    });
                                } else {
                                    n.last_module_click = Some((id, point, time));
                                }
                            }
                        }
                        if !n.view.interaction.is_active() {
                            let ids = n.view.selected_ids();
                            let previous = s["selection"]
                                .as_array()
                                .map(|ids| {
                                    ids.iter().filter_map(|id| id.as_str()).collect::<Vec<_>>()
                                })
                                .unwrap_or_else(|| s["selected"].as_str().into_iter().collect());
                            if ids.iter().map(String::as_str).collect::<Vec<_>>() != previous {
                                let request = studio_service::Request {
                                    method: "view.command".into(),
                                    args: json!({"method":"view.select_nodes","args":{"expected_revision":rev,"ids":ids}}),
                                    actor: "native".into(),
                                };
                                if let Err(error) = studio_service::call(&root.0, &request) {
                                    eprintln!("Native selection: {error}");
                                    n.selection_cursor.clear();
                                }
                                super::sync::wake(&self.app);
                                let _ = self.app.emit("design-changed", ());
                            }
                        }
                        let request = if ops.is_empty() {
                            None
                        } else {
                            Some(studio_service::Request {
                                method: "design.apply_patch".into(),
                                args: json!({"expected_revision":rev,"label":"Native gesture","operations":ops}),
                                actor: "native".into(),
                            })
                        };
                        if let Some(request) = request {
                            let request = studio_service::Request {
                                method: "view.command".into(),
                                args: json!({"method":request.method,"args":request.args}),
                                actor: request.actor,
                            };
                            match studio_service::call(&root.0, &request) {
                                Ok(result) => {
                                    if let Some(revision) =
                                        result["revision"].as_u64().filter(|r| *r > rev)
                                    {
                                        n.pending_revision = Some((
                                            snapshot
                                                .cursor
                                                .rsplit_once(':')
                                                .map(|(e, _)| e)
                                                .unwrap_or("")
                                                .into(),
                                            revision,
                                        ));
                                    }
                                    if moving {
                                        super::measurement::commit(
                                            &self.app,
                                            result["revision"].as_u64().unwrap_or(rev + 1),
                                            started,
                                            doc.nodes.len(),
                                        );
                                    }
                                    super::sync::wake(&self.app);
                                }
                                Err(e) => eprintln!("{e}"),
                            }
                            let _ = self.app.emit("design-changed", ());
                        }
                    }
                    Err(e) => {
                        eprintln!("Native gesture: {e}");
                        n.view.interaction = Default::default();
                        n.selection_cursor.clear();
                    }
                }
            }
        }
        drop(n);
        if drag_motion {
            // Process every motion event, then submit one frame for the last
            // preview position when this batch of window events has drained.
            self.drag_frame_pending = true;
        } else {
            self.drag_frame_pending = false;
            redraw(&self.app);
        }
        false
    }
}

#[cfg(test)]
mod click_tests {
    use super::ClickTime;
    use std::time::{Duration, Instant};

    #[test]
    fn appkit_timestamps_preserve_click_interval_when_dispatch_is_delayed() {
        let started = Instant::now();
        let first = ClickTime {
            received: started,
            appkit: Some(10.0),
            system_double_click: false,
        };
        let delayed = ClickTime {
            received: started + Duration::from_secs(1),
            appkit: Some(10.25),
            system_double_click: false,
        };
        assert!(delayed.follows(&first, 0.5));
        let separate = ClickTime {
            appkit: Some(11.0),
            ..delayed
        };
        assert!(!separate.follows(&first, 0.5));
    }
}
