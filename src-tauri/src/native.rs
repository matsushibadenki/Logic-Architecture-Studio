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
    pub graph_rect: [f32; 4],
    pub dirty: bool,
    pub source: Option<Arc<design_core::Document>>,
    pub frame: Option<FrameStamp>,
    pub selection_cursor: String,
    pub pending_revision: Option<(String, u64)>,
}
pub type Shared = Arc<Mutex<Native>>;
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
        n.view.viewport.zoom = (rect[2] / 1160.).min(rect[3] / 740.).clamp(0.02, 16.);
        n.view.viewport.origin = [
            -30. - rect[0] / n.view.viewport.zoom,
            -30. - rect[1] / n.view.viewport.zoom,
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
            n.view.selection = s["selected"]
                .as_str()
                .and_then(|id| n.view.ids.get(id))
                .copied()
                .into_iter()
                .collect();
            n.selection_cursor = snapshot.cursor.clone();
        }
    }
    n.dirty = false;
    n.frame = Some(frame);
    n.renderer.resize(size.width, size.height);
    n.graph_rect = rect;
    n.view.viewport.size = [size.width as f32, size.height as f32];
    let Native { renderer, view, .. } = &mut *n;
    let result = renderer.draw_editor(
        doc,
        s["locale"].as_str().unwrap_or("ja"),
        view,
        [size.width, size.height],
    );
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
}
impl PluginBuilder<EventLoopMessage> for Bridge {
    type Plugin = Input;
    fn build(self, _: Context<EventLoopMessage>) -> Input {
        Input { app: self.0 }
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
        n.dirty |= !matches!(event, WindowEvent::CursorMoved { .. }) || n.down;
        let mut input = None;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let p = [position.x as f32, position.y as f32];
                if p[0] >= n.graph_rect[0]
                    && p[1] >= n.graph_rect[1]
                    && p[0] < n.graph_rect[0] + n.graph_rect[2]
                    && p[1] < n.graph_rect[1] + n.graph_rect[3]
                {
                    n.position = Some(p);
                    if n.down {
                        input = Some(PointerEvent::Move {
                            pointer: 0,
                            position: p,
                        });
                    }
                } else {
                    input = Some(PointerEvent::Cancel);
                    n.position = None;
                    n.down = false;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(p) = n.position {
                    if *state == ElementState::Pressed {
                        n.down = true;
                        input = Some(PointerEvent::Down {
                            pointer: 0,
                            position: p,
                            button: if *button == MouseButton::Left {
                                PointerButton::Primary
                            } else {
                                PointerButton::Pan
                            },
                            additive: false,
                        });
                    } else if n.down {
                        n.down = false;
                        input = Some(PointerEvent::Up {
                            pointer: 0,
                            position: p,
                        });
                    }
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
            WindowEvent::Focused(false) | WindowEvent::CursorLeft { .. } => {
                n.down = false;
                input = Some(PointerEvent::Cancel);
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
                        let request = if ops.is_empty() {
                            n.view
                                .selected()
                                .filter(|id| Some(id.as_str()) != s["selected"].as_str())
                                .filter(|_| !n.view.interaction.is_active())
                                .map(|id| studio_service::Request {
                                    method: "view.focus_module".into(),
                                    args: json!({"id":id}),
                                    actor: "native".into(),
                                })
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
                    }
                }
            }
        }
        drop(n);
        redraw(&self.app);
        false
    }
}
