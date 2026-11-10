//! Opt-in live-app diagnostics. No synthetic UI events or workspace mutations.
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex, time::Instant};
use tauri::Manager;
pub struct Trace {
    path: Option<PathBuf>,
    pub full_sync: bool,
    samples: Mutex<BTreeMap<u64, Sample>>,
}
struct Sample {
    start: Instant,
    commit: f64,
    native: Option<f64>,
    web: BTreeMap<String, Value>,
    nodes: usize,
    native_transfer: Option<Value>,
}
impl Default for Trace {
    fn default() -> Self {
        Self {
            path: std::env::var_os("LAS_UI_TRACE").map(PathBuf::from),
            full_sync: std::env::var_os("LAS_WEB_SYNC_FULL").is_some(),
            samples: Mutex::new(BTreeMap::new()),
        }
    }
}
fn elapsed(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.
}
#[tauri::command]
pub fn measurement_enabled(app: tauri::AppHandle) -> bool {
    app.state::<Trace>().path.is_some()
}
pub fn commit(app: &tauri::AppHandle, revision: u64, start: Instant, nodes: usize) {
    let trace = app.state::<Trace>();
    if trace.path.is_none() {
        return;
    }
    let mut samples = trace.samples.lock().unwrap();
    samples.insert(
        revision,
        Sample {
            start,
            commit: elapsed(start),
            native: None,
            web: BTreeMap::new(),
            nodes,
            native_transfer: None,
        },
    );
    while samples.len() > 256 {
        samples.pop_first();
    }
}
pub fn native_transfer(app: &tauri::AppHandle, revision: u64, response: &Value) {
    let trace = app.state::<Trace>();
    if trace.path.is_none() {
        return;
    }
    if let Some(sample) = trace.samples.lock().unwrap().get_mut(&revision) {
        sample.native_transfer = Some(
            json!({"kind":response["kind"],"body_bytes":serde_json::to_vec(response).map_or(0,|b|b.len())}),
        );
    };
}
pub fn native(app: &tauri::AppHandle, revision: u64) {
    let trace = app.state::<Trace>();
    if trace.path.is_none() {
        return;
    }
    if let Some(s) = trace.samples.lock().unwrap().get_mut(&revision) {
        s.native.get_or_insert_with(|| elapsed(s.start));
    }
    save(app);
}
#[tauri::command]
pub fn web_frame_ack(
    app: tauri::AppHandle,
    label: String,
    revision: u64,
    bytes: usize,
    kind: String,
) {
    if !["controls", "library", "inspector", "results"].contains(&label.as_str()) {
        return;
    }
    let trace = app.state::<Trace>();
    if trace.path.is_none() {
        return;
    }
    if let Some(s) = trace.samples.lock().unwrap().get_mut(&revision) {
        let duration = elapsed(s.start);
        s.web.entry(label).or_insert_with(
            || json!({"paint_opportunity_ms":duration,"sync_body_bytes":bytes,"kind":kind}),
        );
    }
    save(&app);
}
fn save(app: &tauri::AppHandle) {
    let a = app.clone();
    std::thread::spawn(move || {
        let trace = a.state::<Trace>();
        let Some(path) = &trace.path else {
            return;
        };
        let samples = trace.samples.lock().unwrap();
        let values:Vec<_>=samples.iter().map(|(rev,s)|json!({"revision":rev,"nodes":s.nodes,"commit_ms":s.commit,"native_present_request_ms":s.native,"webviews":s.web,"native_sync":s.native_transfer})).collect();
        let output = json!({"profile":if cfg!(debug_assertions) {"debug"} else {"release"},"web_sync":if trace.full_sync {"full"} else {"delta"},"samples":values});
        if let Ok(bytes) = serde_json::to_vec_pretty(&output) {
            let _ = std::fs::write(path, bytes);
        }
    });
}
