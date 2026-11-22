//! Opt-in, in-process input replay on the normal window renderer.
use crate::{native, sync};
use block2::RcBlock;
use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, ClassType};
use objc2_app_kit::NSView;
use objc2_metal::MTLDrawable;
use objc2_quartz_core::{CACurrentMediaTime, CAMetalDrawable, CAMetalLayer};
use serde::Serialize;
use std::{
    cell::RefCell,
    path::PathBuf,
    ptr::NonNull,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use studio_renderer::unge_interaction::{PointerButton, PointerEvent};
use tauri::Manager;

const COUNT: usize = 250;
const PERIOD: Duration = Duration::from_millis(4);
const MOVED: [&str; 4] = ["n0", "n512", "n1024", "n1536"];

#[derive(Clone)]
struct Frame {
    input: Input,
    delivered_at: f64,
    presented: mpsc::Sender<Sample>,
    in_flight: Option<Arc<AtomicBool>>,
}

#[derive(Clone, Copy, Serialize)]
struct Input {
    index: usize,
    offset: [f32; 2],
    input_at: f64,
}

#[derive(Serialize)]
struct Presented {
    input: usize,
    drawable_id: usize,
    delivered_at: f64,
    presented_at: f64,
}

#[derive(Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum Sample {
    Presented(Presented),
    Cpu {
        input: usize,
        assembly_ms: f64,
        surface: studio_renderer::CpuSurfaceFrameTimings,
    },
    Gpu {
        input: usize,
        start_at: f64,
        end_at: f64,
    },
}

pub fn gpu_observer() -> Option<studio_renderer::GpuExecutionObserver> {
    FRAME.with(|slot| slot.borrow().clone()).map(|frame| {
        Box::new(move |start_at, end_at| {
            let _ = frame.presented.send(Sample::Gpu {
                input: frame.input.index,
                start_at,
                end_at,
            });
        }) as studio_renderer::GpuExecutionObserver
    })
}

pub fn cpu_sample(assembly_ms: f64, surface: &studio_renderer::CpuSurfaceFrameTimings) {
    FRAME.with(|slot| {
        if let Some(frame) = slot.borrow().as_ref() {
            let _ = frame.presented.send(Sample::Cpu {
                input: frame.input.index,
                assembly_ms,
                surface: surface.clone(),
            });
        }
    });
}

thread_local! {
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(CAMetalLayer))]
    #[name = "LASMeasuredMetalLayer"]
    struct MeasuredMetalLayer;

    impl MeasuredMetalLayer {
        #[unsafe(method_id(nextDrawable))]
        fn next_drawable(&self) -> Option<Retained<ProtocolObject<dyn CAMetalDrawable>>> {
            // SAFETY: Match CAMetalLayer's own method signature and ownership.
            let drawable: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>> =
                unsafe { msg_send![super(self), nextDrawable] };
            if let Some((drawable, frame)) = drawable.as_ref().and_then(|drawable| {
                FRAME.with(|slot| slot.borrow_mut().as_mut().map(|frame| {
                    let mut acquired = frame.clone();
                    acquired.in_flight = frame.in_flight.take();
                    (drawable, acquired)
                }))
            }) {
                let handler = RcBlock::new(move |drawable: NonNull<ProtocolObject<dyn MTLDrawable>>| {
                    // SAFETY: Metal keeps the drawable alive for this callback.
                    let drawable = unsafe { drawable.as_ref() };
                    let presented_at = drawable.presentedTime();
                    if let Some(in_flight) = &frame.in_flight {
                        in_flight.store(false, Ordering::Release);
                    }
                    let _ = frame.presented.send(Sample::Presented(Presented {
                        input: frame.input.index,
                        drawable_id: drawable.drawableID(),
                        delivered_at: frame.delivered_at,
                        presented_at,
                    }));
                });
                // SAFETY: The block is copied by Metal. It captures only Send data.
                unsafe { drawable.addPresentedHandler(RcBlock::as_ptr(&handler)) };
            }
            drawable
        }
    }
);

pub fn install(window: &tauri::Window) -> Result<Option<PathBuf>, String> {
    let Some(path) = std::env::var_os("LAS_NATIVE_DRAG_TRACE").map(PathBuf::from) else {
        return Ok(None);
    };
    if std::env::var_os("LAS_WORKSPACE").is_none() {
        return Err("LAS_NATIVE_DRAG_TRACE requires an explicit QA workspace".into());
    }
    let view = window.ns_view().map_err(|error| error.to_string())?;
    // SAFETY: Tauri returns this window's NSView; setup runs on the main thread.
    let view = unsafe { &*view.cast::<NSView>() };
    // SAFETY: +layer constructs an initialized instance of the receiver class.
    let layer: Retained<MeasuredMetalLayer> =
        unsafe { msg_send![MeasuredMetalLayer::class(), layer] };
    view.setLayer(Some(&layer));
    view.setWantsLayer(true);
    Ok(Some(path))
}

#[derive(Default)]
struct Pending {
    input: Option<Input>,
    queued: bool,
}

fn begin(app: &tauri::AppHandle) -> Result<([f32; 2], f32), String> {
    let snapshot = app
        .state::<sync::Cache>()
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or("workspace has not synchronized")?;
    let doc = &snapshot.document;
    if doc.nodes.len() != 2000
        || doc.edges.len() != 3996
        || doc.edges.iter().filter(|edge| edge.from == "n0").count() != 1998
    {
        return Err("expected the 2,000-node wide-fanout QA workspace".into());
    }
    let mut native = app.state::<native::Shared>().inner().lock().unwrap();
    if native.view.selection.len() != MOVED.len()
        || MOVED.iter().any(|id| {
            native
                .view
                .ids
                .get(*id)
                .is_none_or(|id| !native.view.selection.contains(id))
        })
    {
        return Err("expected the four preselected QA nodes".into());
    }
    let node = doc.node("n0").map_err(|error| error.to_string())?;
    let view = native.view.viewport;
    let position = [
        (node.x + design_core::NODE_WIDTH * 0.5 - view.origin[0]) * view.zoom,
        (node.y + design_core::node_height(&node.kind) * 0.5 - view.origin[1]) * view.zoom,
    ];
    native.view.interaction.set_grid_snap(false);
    native.view.pointer(
        doc,
        snapshot.meta["revision"].as_u64().unwrap_or(0),
        PointerEvent::Down {
            pointer: 0,
            position,
            button: PointerButton::Primary,
            additive: false,
        },
    )?;
    Ok((position, view.zoom))
}

fn deliver(
    app: &tauri::AppHandle,
    input: Input,
    anchor: [f32; 2],
    zoom: f32,
    presented: mpsc::Sender<Sample>,
    in_flight: Option<Arc<AtomicBool>>,
) -> Result<(), String> {
    let delivered_at = CACurrentMediaTime();
    let snapshot = app
        .state::<sync::Cache>()
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or("workspace has not synchronized")?;
    {
        let mut native = app.state::<native::Shared>().inner().lock().unwrap();
        native.view.pointer(
            &snapshot.document,
            snapshot.meta["revision"].as_u64().unwrap_or(0),
            PointerEvent::Move {
                pointer: 0,
                position: [
                    anchor[0] + input.offset[0] * zoom,
                    anchor[1] + input.offset[1] * zoom,
                ],
            },
        )?;
        let placement = &native.view.interaction.preview().placement;
        if placement.is_empty() {
            return Ok(()); // Motion below the editor drag threshold.
        }
        if placement.len() != MOVED.len() {
            return Err("input did not produce a four-node drag preview".into());
        }
        native.dirty = true;
    }
    if let Some(in_flight) = &in_flight {
        in_flight.store(true, Ordering::Release);
    }
    FRAME.with(|slot| {
        slot.replace(Some(Frame {
            input,
            delivered_at,
            presented,
            in_flight,
        }))
    });
    native::redraw(app);
    if let Some(flag) = FRAME
        .with(|slot| slot.take())
        .and_then(|frame| frame.in_flight)
    {
        flag.store(false, Ordering::Release);
    }
    Ok(())
}

pub fn start(app: tauri::AppHandle, path: PathBuf) {
    std::thread::spawn(move || {
        let result = replay(&app);
        let report = result.unwrap_or_else(|error| serde_json::json!({"error": error}));
        if let Err(error) = std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()) {
            eprintln!("Native drag trace: {error}");
        }
    });
}

#[derive(Serialize)]
struct Refresh {
    callback_at: f64,
    target_at: f64,
}

struct DisplayClock(objc2_core_foundation::CFRetained<objc2_core_video::CVDisplayLink>);

#[allow(deprecated)] // CoreVideo runs off the UI thread, even while the window is busy.
impl DisplayClock {
    fn start(display: u32, samples: mpsc::SyncSender<Refresh>) -> Result<Self, String> {
        use objc2_core_video::{
            CVDisplayLink, CVGetHostClockFrequency, CVTimeStamp, CVTimeStampFlags,
        };
        let mut link = std::ptr::null_mut();
        let code =
            unsafe { CVDisplayLink::create_with_cg_display(display, NonNull::from(&mut link)) };
        if code != 0 {
            return Err(format!("CoreVideo display link creation failed: {code}"));
        }
        let link = NonNull::new(link).ok_or("CoreVideo returned a null display link")?;
        // SAFETY: CoreVideo's Create API transfers one retain count.
        let link = unsafe { objc2_core_foundation::CFRetained::from_raw(link) };
        let frequency = CVGetHostClockFrequency();
        let handler = RcBlock::new(
            move |_: NonNull<CVDisplayLink>,
                  _: NonNull<CVTimeStamp>,
                  output: NonNull<CVTimeStamp>,
                  _: u64,
                  _: NonNull<u64>|
                  -> i32 {
                // SAFETY: This pointer is valid for the duration of CoreVideo's callback.
                let output = unsafe { output.as_ref() };
                if output.flags & CVTimeStampFlags::HostTimeValid.bits() != 0 {
                    let _ = samples.try_send(Refresh {
                        callback_at: CACurrentMediaTime(),
                        target_at: output.hostTime as f64 / frequency,
                    });
                }
                0
            },
        );
        // SAFETY: CoreVideo copies the valid block, whose captures are Send.
        let code = unsafe { link.set_output_handler(RcBlock::as_ptr(&handler)) };
        if code != 0 {
            return Err(format!("CoreVideo callback registration failed: {code}"));
        }
        let code = link.start();
        if code != 0 {
            return Err(format!("CoreVideo display link did not start: {code}"));
        }
        Ok(Self(link))
    }
}

impl Drop for DisplayClock {
    #[allow(deprecated)]
    fn drop(&mut self) {
        self.0.stop();
    }
}

fn replay(app: &tauri::AppHandle) -> Result<serde_json::Value, String> {
    // Allow the initial workspace snapshot, viewport and dock layout to settle.
    std::thread::sleep(Duration::from_secs(3));
    let (focused, focus_result) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = handle
            .get_window("workspace")
            .ok_or_else(|| "workspace window is missing".to_owned())
            .and_then(|window| window.set_focus().map_err(|error| error.to_string()));
        let _ = focused.send(result);
    })
    .map_err(|error| error.to_string())?;
    focus_result
        .recv_timeout(Duration::from_secs(10))
        .map_err(|error| error.to_string())??;
    std::thread::sleep(Duration::from_millis(250));
    let (started, ready) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        native::redraw(&handle);
        let _ = started.send(begin(&handle));
    })
    .map_err(|error| error.to_string())?;
    let (anchor, zoom) = ready
        .recv_timeout(Duration::from_secs(10))
        .map_err(|error| error.to_string())??;
    let window = app
        .get_window("workspace")
        .ok_or("missing workspace window")?;
    let current = window.current_monitor().map_err(|e| e.to_string())?;
    let primary = window.primary_monitor().map_err(|e| e.to_string())?;
    if current.as_ref().map(|m| m.position()) != primary.as_ref().map(|m| m.position()) {
        return Err("display timing requires the QA window on the primary monitor".into());
    }
    let display = objc2_core_graphics::CGMainDisplayID();
    let (refresh, refresh_log) = mpsc::sync_channel(1024);
    let clock = DisplayClock::start(display, refresh)?;
    let paced = std::env::var_os("LAS_NATIVE_DRAG_PRESENT_PACED").is_some();
    let in_flight = paced.then(|| Arc::new(AtomicBool::new(false)));
    let pending = Arc::new(Mutex::new(Pending::default()));
    let (presented, frames_receiver) = mpsc::channel();
    let (errors, failures) = mpsc::channel();
    let start = Instant::now();
    let mut inputs = Vec::with_capacity(COUNT);
    for index in 0..COUNT {
        let phase = (index % 100) as f32;
        let distance = 50. - (phase - 50.).abs();
        let deadline = start + PERIOD * index as u32;
        if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
            std::thread::sleep(delay);
        }
        let input = Input {
            index,
            offset: [distance * 1.5, distance * 0.75],
            input_at: CACurrentMediaTime(),
        };
        inputs.push(input);
        {
            let mut pending = pending.lock().unwrap();
            pending.input = Some(input);
            if pending.queued
                || in_flight
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                continue;
            }
            pending.queued = true;
        }
        let pending = pending.clone();
        let handle = app.clone();
        let presented = presented.clone();
        let errors = errors.clone();
        let in_flight = in_flight.clone();
        app.run_on_main_thread(move || {
            let input = {
                let mut pending = pending.lock().unwrap();
                pending.queued = false;
                if in_flight
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Acquire))
                {
                    return;
                }
                pending.input.take()
            };
            if let Some(input) = input {
                if let Err(error) = deliver(&handle, input, anchor, zoom, presented, in_flight) {
                    let _ = errors.send(error);
                }
            }
        })
        .map_err(|error| error.to_string())?;
    }
    let (finished, done) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let shared = handle.state::<native::Shared>();
        let mut native = shared.lock().unwrap();
        native.view.interaction = Default::default();
        native.dirty = true;
        drop(native);
        native::redraw(&handle);
        let _ = finished.send(());
    })
    .map_err(|error| error.to_string())?;
    done.recv_timeout(Duration::from_secs(10))
        .map_err(|error| error.to_string())?;
    std::thread::sleep(Duration::from_millis(350));
    let (mut frames, mut stages) = (Vec::new(), Vec::new());
    for sample in frames_receiver.try_iter() {
        match sample {
            Sample::Presented(frame) => frames.push(frame),
            sample => stages.push(sample),
        }
    }
    let failures: Vec<_> = failures.try_iter().collect();
    drop(clock);
    let refresh: Vec<_> = refresh_log.try_iter().collect();
    let (width, height) = app
        .get_window("workspace")
        .and_then(|window| window.inner_size().ok())
        .map(|size| (size.width, size.height))
        .unwrap_or_default();
    Ok(
        serde_json::json!({"nodes": 2000, "moved": MOVED, "target_hz": 250,
        "canvas_window_px": [width, height], "zoom":zoom, "inputs": inputs, "frames": frames, "stages": stages, "errors": failures,
        "refresh": refresh, "display": display, "present_paced": paced,
        "frame_latency": if std::env::var_os("LAS_NATIVE_DRAG_SHALLOW_QUEUE").is_some() { 1 } else { 2 }}),
    )
}
