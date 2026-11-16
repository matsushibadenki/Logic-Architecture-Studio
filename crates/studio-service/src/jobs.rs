//! Workspace-local jobs for long-running EDA operations.
use crate::{Request, Service};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const METHODS: &[&str] = &[
    "verify.rtl",
    "verify.formal",
    "simulation.rtl_testbench",
    "synthesis.run",
];

#[derive(Default)]
pub(crate) struct Jobs(Mutex<VecDeque<Arc<Job>>>);

struct Job {
    id: String,
    method: String,
    revision: u64,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    cancellation_requested: bool,
    publishing: bool,
    stage: Option<String>,
    result: Option<Result<Value, String>>,
}

// External adapters run synchronously on this dedicated worker. Keeping
// cancellation scoped here also leaves direct, blocking callers supported.
thread_local! {
    static CURRENT: RefCell<Option<Arc<Job>>> = const { RefCell::new(None) };
}

pub(crate) fn cancelled() -> bool {
    CURRENT.with_borrow(|job| {
        job.as_ref()
            .is_some_and(|job| job.state.lock().unwrap().cancellation_requested)
    })
}

pub(crate) fn checkpoint() -> Result<(), String> {
    if cancelled() {
        Err("Job cancelled".into())
    } else {
        Ok(())
    }
}

pub(crate) fn stage(name: &str) {
    CURRENT.with_borrow(|job| {
        if let Some(job) = job {
            job.state.lock().unwrap().stage = Some(name.into());
        }
    });
}

/// Once a result starts changing shared application state, cancellation can
/// no longer win the race. Check before publishing, not after mutating.
pub(crate) fn begin_publish() -> Result<(), String> {
    CURRENT.with_borrow(|job| {
        if let Some(job) = job {
            let mut state = job.state.lock().unwrap();
            if state.cancellation_requested {
                return Err("Job cancelled".into());
            }
            state.publishing = true;
        }
        Ok(())
    })
}

impl Job {
    fn snapshot(&self, state: &State) -> Value {
        let status = match (&state.result, state.cancellation_requested) {
            (Some(_), true) => "cancelled",
            (None, true) => "cancelling",
            (None, false) => "running",
            (Some(Ok(_)), false) => "completed",
            (Some(Err(_)), false) => "failed",
        };
        let mut value = json!({
            "id": self.id, "method": self.method, "revision": self.revision,
            "status": status, "stage": state.stage,
            "cancellable": state.result.is_none() && !state.cancellation_requested && !state.publishing,
        });
        if !state.cancellation_requested {
            match &state.result {
                Some(Ok(result)) => value["result"] = result.clone(),
                Some(Err(error)) => value["error"] = json!(error),
                None => {}
            }
        }
        value
    }
}

impl Jobs {
    pub(crate) fn start(&self, service: &Service, req: Request) -> Result<Value, String> {
        let method = req.args["method"].as_str().ok_or("Missing job method")?;
        if !METHODS.contains(&method) {
            return Err("Unsupported job method".into());
        }
        let mut args = req.args["args"].clone();
        let revision = service.state.lock().unwrap().revision;
        if method == "synthesis.run" && args["expected_revision"].is_null() {
            args["expected_revision"] = json!(revision);
        }
        crate::validate_request(method, &args)?;
        if args["expected_revision"].as_u64() != Some(revision) {
            return Err("Revision conflict".into());
        }

        let mut jobs = self.0.lock().unwrap();
        if jobs
            .iter()
            .any(|job| job.state.lock().unwrap().result.is_none())
        {
            return Err("An EDA job is already running".into());
        }
        // Keep a bounded set of completed results in memory for status polling.
        let job = Arc::new(Job {
            id: format!(
                "job-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            ),
            method: method.into(),
            revision,
            state: Mutex::new(State::default()),
        });
        let response = job.snapshot(&job.state.lock().unwrap());
        let worker = job.clone();
        let service = service.clone();
        std::thread::Builder::new()
            .name("eda-job".into())
            .spawn(move || {
                CURRENT.set(Some(worker.clone()));
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    checkpoint()?;
                    service.execute(Request {
                        method: worker.method.clone(),
                        args,
                        actor: req.actor,
                    })
                }))
                .unwrap_or_else(|_| Err("EDA job worker failed".into()));
                let mut state = worker.state.lock().unwrap();
                state.stage = None;
                state.result = Some(if state.cancellation_requested {
                    Err("Job cancelled".into())
                } else {
                    result
                });
            })
            .map_err(|e| e.to_string())?;
        jobs.push_back(job);
        if jobs.len() > 16 {
            jobs.pop_front();
        }
        Ok(response)
    }

    pub(crate) fn inspect(&self, args: &Value, cancel: bool) -> Result<Value, String> {
        let id = args["id"].as_str().ok_or("Missing job ID")?;
        let job = self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|job| job.id == id)
            .cloned()
            .ok_or("Unknown or expired job ID")?;
        let mut state = job.state.lock().unwrap();
        if cancel && state.result.is_none() && !state.publishing {
            state.cancellation_requested = true;
        }
        Ok(job.snapshot(&state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "Requires installed Verilator, a C++ compiler and Yosys"]
    fn real_verification_and_synthesis_cancel_then_rerun() {
        let dir = std::env::temp_dir().join(format!("las-jobs-{}", std::process::id()));
        let service = Service::open(&dir).unwrap();
        let invoke = |method: &str, args| {
            service
                .execute(Request {
                    method: method.into(),
                    args,
                    actor: "test".into(),
                })
                .unwrap()
        };
        for (method, args, stage) in [
            (
                "verify.rtl",
                json!({"expected_revision":0,"engine":"verilator"}),
                "compile.log",
            ),
            ("synthesis.run", json!({"expected_revision":0}), "yosys.log"),
        ] {
            let start = || invoke("job.start", json!({"method":method,"args":args}));
            let first = start();
            assert_eq!(first["status"], "running");
            let id = &first["id"];
            let status = || invoke("job.status", json!({"id":id}));
            let limit = Instant::now() + Duration::from_secs(5);
            loop {
                let current = status();
                assert_eq!(current["status"], "running", "{current}");
                if current["stage"] == stage {
                    break;
                }
                assert!(Instant::now() < limit, "{current}");
                std::thread::sleep(Duration::from_millis(1));
            }
            let cancel = invoke("job.cancel", json!({"id":id}));
            assert_eq!(cancel["status"], "cancelling");
            let limit = Instant::now() + Duration::from_secs(5);
            loop {
                let current = status();
                if current["status"] == "cancelled" {
                    assert!(current.get("result").is_none());
                    break;
                }
                assert_eq!(current["status"], "cancelling", "{current}");
                assert!(Instant::now() < limit, "{current}");
                std::thread::sleep(Duration::from_millis(5));
            }
            let second = start();
            assert_ne!(second["id"], *id);
            let limit = Instant::now() + Duration::from_secs(40);
            loop {
                let current = invoke("job.status", json!({"id":second["id"]}));
                if current["status"] == "completed" {
                    assert_eq!(current["result"]["success"], true, "{current}");
                    break;
                }
                assert_eq!(current["status"], "running", "{current}");
                assert!(Instant::now() < limit, "{current}");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        drop(service);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
