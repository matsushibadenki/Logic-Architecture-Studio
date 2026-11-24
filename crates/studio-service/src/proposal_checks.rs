use crate::Service;
use design_core::{apply, testbench::Testbench, Document, Patch};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    patch: Patch,
    expected_version: u64,
}

fn run(document: &Document, bench: &Testbench) -> Value {
    match bench.run(document) {
        Ok((_, _, mut report)) => {
            report.failures.truncate(5);
            json!({"report":report})
        }
        Err(error) => json!({"error":error}),
    }
}

pub(crate) fn compare(service: &Service, args: Value) -> Result<Value, String> {
    let Request {
        patch,
        expected_version,
    } = serde_json::from_value(args).map_err(|err| err.to_string())?;
    let (document, bench) = {
        let state = service.state.lock().map_err(|err| err.to_string())?;
        if state.revision != patch.expected_revision {
            return Err("The circuit revision changed. Generate a new proposal.".into());
        }
        if state.testbench.version != expected_version {
            return Err("The stimulus table changed. Run the comparison again.".into());
        }
        (state.document.clone(), state.testbench.clone())
    };
    if !bench.rows.iter().any(|row| !row.expected.is_empty()) {
        return Err("Save stimulus rows with expected outputs before comparing a proposal.".into());
    }
    let candidate = apply(&document, &patch.operations)?;
    Ok(json!({
        "revision":patch.expected_revision,
        "testbench_version":bench.version,
        "baseline":run(&document, &bench),
        "candidate":run(&candidate, &bench)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_saved_assertions_without_changing_the_workspace() {
        let root = std::env::temp_dir().join(format!("las-proposal-check-{}", std::process::id()));
        let service = Service::open(&root).unwrap();
        let command = |method: &str, args| {
            service.execute(crate::Request {
                method: method.into(),
                args,
                actor: "test".into(),
            })
        };
        command(
            "testbench.import_csv",
            json!({"expected_revision":0,"expected_version":0,
                "source":"reset,in.a,in.b,in.op,expect.result\n0,3,2,0,5\n"}),
        )
        .unwrap();
        let args = json!({"expected_version":1,"patch":{
            "expected_revision":0,"label":"Try subtract",
            "operations":[{"op":"update_node","id":"add","kind":"sub"}]
        }});
        let before = command("project.inspect", json!({})).unwrap();
        let result = command("design.check_proposal", args.clone()).unwrap();
        assert_eq!(result["baseline"]["report"]["passed"], 1);
        assert_eq!(result["candidate"]["report"]["passed"], 0);
        assert_eq!(result["candidate"]["report"]["failures"][0]["actual"], "1");
        assert_eq!(command("project.inspect", json!({})).unwrap(), before);

        let mut stale = args;
        stale["expected_version"] = json!(0);
        assert!(command("design.check_proposal", stale.clone()).is_err());
        let mut apply = json!({
            "method":"design.apply_patch",
            "args":stale["patch"],
            "expected_testbench_version":0
        });
        assert!(command("view.command", apply.clone()).is_err());
        assert_eq!(command("project.inspect", json!({})).unwrap(), before);
        apply["expected_testbench_version"] = json!(1);
        command("view.command", apply).unwrap();
        assert_eq!(
            command("project.inspect", json!({})).unwrap()["revision"],
            1
        );

        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }
}
