//! Durable stimulus definitions; simulation traces remain ephemeral.
use crate::{CycleInput, CycleSample, Document, Kind, Node};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CSV_BYTES: usize = 256 * 1024;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Testbench {
    pub version: u64,
    pub rows: Vec<Stimulus>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stimulus {
    pub reset: bool,
    pub inputs: BTreeMap<String, String>,
    #[serde(default)]
    pub expected: BTreeMap<String, String>,
}
#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub total: usize,
    pub passed: usize,
    pub failures: Vec<CheckFailure>,
}
#[derive(Debug, Serialize)]
pub struct CheckFailure {
    pub cycle: usize,
    pub signal: String,
    pub expected: String,
    pub actual: String,
}
fn signals(doc: &Document, kind: Kind) -> BTreeMap<String, &Node> {
    doc.nodes
        .iter()
        .filter(|n| n.kind == kind)
        .map(|n| (n.name.clone(), n))
        .collect()
}
impl Testbench {
    pub fn validate_rows(doc: &Document, rows: &[Stimulus]) -> Result<(), String> {
        if rows.is_empty() || rows.len() > 256 {
            return Err("Supply 1–256 stimulus rows".into());
        }
        if serde_json::to_vec(rows).map_err(|e| e.to_string())?.len() > MAX_CSV_BYTES {
            return Err("Stimulus table exceeds 256 KiB".into());
        }
        let inputs = signals(doc, Kind::Input);
        let outputs = signals(doc, Kind::Output);
        if doc.nodes.iter().any(|n| n.kind == Kind::Register) && !rows[0].reset {
            return Err("Cycle 1 must assert reset".into());
        }
        for (i, row) in rows.iter().enumerate() {
            if !row.inputs.keys().eq(inputs.keys()) {
                return Err(format!(
                    "Cycle {}: supply exactly the current design inputs",
                    i + 1
                ));
            }
            for (name, text) in &row.inputs {
                inputs[name]
                    .parse_value(text)
                    .map_err(|e| format!("Cycle {}, input {name}: {e}", i + 1))?;
            }
            for (name, text) in &row.expected {
                let width = outputs
                    .get(name)
                    .ok_or_else(|| format!("Cycle {}: unknown output {name}", i + 1))?;
                width
                    .parse_value(text)
                    .map_err(|e| format!("Cycle {}, expected {name}: {e}", i + 1))?;
            }
        }
        Ok(())
    }
    pub fn run(
        &self,
        doc: &Document,
    ) -> Result<(Vec<CycleInput>, Vec<CycleSample>, CheckReport), String> {
        Self::validate_rows(doc, &self.rows)?;
        doc.validate(true)?;
        if self.rows.len().saturating_mul(doc.nodes.len()) > 524_288 {
            return Err("Trace exceeds 524288 node samples; use fewer cycles".into());
        }
        let cycles: Vec<_> = self
            .rows
            .iter()
            .map(|r| CycleInput {
                reset: r.reset,
                inputs: r.inputs.clone(),
            })
            .collect();
        let samples = if doc.nodes.iter().any(|n| n.kind == Kind::Register) {
            doc.simulate_cycles(&cycles)?
        } else {
            cycles
                .iter()
                .map(|cycle| {
                    let inputs = doc.parse_signals(&cycle.inputs, Kind::Input)?;
                    let values = doc.evaluate_signals(&inputs)?;
                    let outputs = doc
                        .nodes
                        .iter()
                        .filter(|n| n.kind == Kind::Output)
                        .map(|n| (n.name.clone(), n.decimal(values[&n.id])))
                        .collect();
                    Ok(CycleSample {
                        reset: cycle.reset,
                        values,
                        outputs,
                        registers: BTreeMap::new(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        let mut report = CheckReport {
            total: 0,
            passed: 0,
            failures: vec![],
        };
        for (i, (row, sample)) in self.rows.iter().zip(&samples).enumerate() {
            for (name, expected) in &row.expected {
                report.total += 1;
                let actual = &sample.outputs[name];
                let node = signals(doc, Kind::Output)[name];
                if node.parse_value(expected)? == node.parse_value(actual)? {
                    report.passed += 1;
                } else {
                    report.failures.push(CheckFailure {
                        cycle: i,
                        signal: name.clone(),
                        expected: expected.clone(),
                        actual: actual.clone(),
                    });
                }
            }
        }
        Ok((cycles, samples, report))
    }
    pub fn import_csv(doc: &Document, source: &str) -> Result<Vec<Stimulus>, String> {
        if source.len() > MAX_CSV_BYTES {
            return Err("CSV exceeds 256 KiB".into());
        }
        let records = csv_records(source.trim_start_matches('\u{feff}'))?;
        let header = records.first().ok_or("CSV header is missing")?;
        let inputs = signals(doc, Kind::Input);
        let outputs = signals(doc, Kind::Output);
        let mut seen = BTreeSet::new();
        for col in header {
            if !seen.insert(col.as_str()) {
                return Err(format!("Duplicate CSV column: {col}"));
            }
            if col != "cycle"
                && col != "reset"
                && !col
                    .strip_prefix("in.")
                    .is_some_and(|n| inputs.contains_key(n))
                && !col
                    .strip_prefix("expect.")
                    .is_some_and(|n| outputs.contains_key(n))
            {
                return Err(format!("Unknown CSV column: {col}"));
            }
        }
        if !seen.contains("reset")
            || inputs
                .keys()
                .any(|n| !seen.contains(format!("in.{n}").as_str()))
        {
            return Err("CSV requires reset and every in.<input> column".into());
        }
        let mut rows = vec![];
        for (i, fields) in records.iter().skip(1).enumerate() {
            if fields.len() != header.len() {
                return Err(format!("CSV row {} has the wrong column count", i + 2));
            }
            let mut row = Stimulus {
                reset: false,
                inputs: BTreeMap::new(),
                expected: BTreeMap::new(),
            };
            for (col, cell) in header.iter().zip(fields) {
                if col == "cycle" {
                    if cell.parse::<usize>().ok() != Some(i + 1) {
                        return Err(format!("CSV cycle must be {}", i + 1));
                    }
                } else if col == "reset" {
                    row.reset = match cell.as_str() {
                        "1" | "true" => true,
                        "0" | "false" => false,
                        _ => return Err(format!("Cycle {}: reset must be 0 or 1", i + 1)),
                    };
                } else if let Some(name) = col.strip_prefix("in.") {
                    row.inputs.insert(name.into(), cell.clone());
                } else if let Some(name) = col.strip_prefix("expect.") {
                    if !cell.is_empty() {
                        row.expected.insert(name.into(), cell.clone());
                    }
                }
            }
            rows.push(row);
        }
        Self::validate_rows(doc, &rows)?;
        Ok(rows)
    }
    pub fn export_csv(&self, doc: &Document) -> Result<String, String> {
        Self::validate_rows(doc, &self.rows)?;
        let inputs = signals(doc, Kind::Input);
        let outputs = signals(doc, Kind::Output);
        let mut header = vec!["cycle".into(), "reset".into()];
        header.extend(inputs.keys().map(|n| format!("in.{n}")));
        header.extend(outputs.keys().map(|n| format!("expect.{n}")));
        let mut lines = vec![header.join(",")];
        for (i, row) in self.rows.iter().enumerate() {
            let mut cells = vec![(i + 1).to_string(), u8::from(row.reset).to_string()];
            cells.extend(inputs.keys().map(|n| row.inputs[n].clone()));
            cells.extend(
                outputs
                    .keys()
                    .map(|n| row.expected.get(n).cloned().unwrap_or_default()),
            );
            lines.push(cells.join(","));
        }
        Ok(lines.join("\r\n") + "\r\n")
    }
}
// Strict CSV quoting, including CRLF, escaped quotes and quoted records. Values
// are subsequently validated as numbers; no spreadsheet formulas are accepted.
fn csv_records(source: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = vec![];
    let mut row = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                cell.push(c);
            }
        } else if c == ',' || c == '\n' || c == '\r' {
            row.push(cell.trim().to_string());
            cell.clear();
            closed = false;
            if c != ',' {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                records.push(std::mem::take(&mut row));
                if records.len() > 257 {
                    return Err("CSV exceeds 256 cycles".into());
                }
            }
        } else if c == '"' && cell.is_empty() && !closed {
            quoted = true;
        } else if closed || c == '"' {
            return Err("Malformed CSV quoting".into());
        } else {
            cell.push(c);
        }
    }
    if quoted {
        return Err("Unterminated CSV quote".into());
    }
    if closed || !cell.is_empty() || !row.is_empty() {
        row.push(cell.trim().to_string());
        records.push(row);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Workspace;
    use serde_json::json;
    pub fn accumulator() -> Document {
        serde_json::from_value(json!({"name":"accumulator","nodes":[
            {"id":"inc","name":"inc","kind":"input","width":3,"x":0,"y":0},
            {"id":"sum","name":"sum","kind":"add","width":3,"x":400,"y":0},
            {"id":"acc","name":"acc","kind":"register","width":3,"x":800,"y":0},
            {"id":"result","name":"result","kind":"output","width":3,"x":1200,"y":0}],
            "edges":[{"id":"e0","from":"inc","to":"sum","port":"a"},{"id":"e1","from":"acc","to":"sum","port":"b"},{"id":"e2","from":"sum","to":"acc","port":"d"},{"id":"e3","from":"acc","to":"result","port":"a"}]})).unwrap()
    }
    #[test]
    fn csv_round_trip_handles_bom_quotes_crlf_blank_expectations_and_reset() {
        let doc = accumulator();
        let csv = "\u{feff}\"cycle\",\"reset\",\"in.inc\",\"expect.result\"\r\n1,1,0x3,0\r\n2,0,3,3\r\n3,0,3,6\r\n4,0,3,1\r\n5,1,7,0\r\n6,0,2,\r\n";
        let rows = Testbench::import_csv(&doc, csv).unwrap();
        let table = Testbench { version: 1, rows };
        let (_, samples, checks) = table.run(&doc).unwrap();
        assert_eq!((checks.total, checks.passed), (5, 5));
        assert_eq!(samples[3].outputs["result"], "1");
        assert_eq!(samples[5].outputs["result"], "2");
        assert_eq!(
            Testbench::import_csv(&doc, &table.export_csv(&doc).unwrap()).unwrap(),
            table.rows
        );
    }
    #[test]
    fn failed_expectations_report_cycle_signal_expected_and_actual() {
        let doc = accumulator();
        let rows =
            Testbench::import_csv(&doc, "reset,in.inc,expect.result\n1,3,1\n0,3,0x3\n").unwrap();
        let (_, _, checks) = Testbench { version: 1, rows }.run(&doc).unwrap();
        assert_eq!((checks.total, checks.passed), (2, 1));
        assert_eq!(checks.failures[0].cycle, 0);
        assert_eq!(checks.failures[0].signal, "result");
        assert_eq!(checks.failures[0].expected, "1");
        assert_eq!(checks.failures[0].actual, "0");
    }
    #[test]
    fn invalid_csv_is_rejected_without_silent_truncation_or_column_loss() {
        let doc = accumulator();
        for csv in [
            "reset,in.inc\n0,3",
            "reset,in.inc\n1,8",
            "reset,in.inc\n1,-1",
            "reset,in.inc\n1,",
            "reset,in.inc,in.inc\n1,3,3",
            "reset,in.other\n1,3",
            "reset,in.inc,expect.nope\n1,3,0",
            "reset,in.inc,expect.result\n1,3,8",
            "reset,in.inc\n2,3",
            "reset,in.inc\n1,3,4",
            "reset,in.inc\n1,\"3",
            "reset,in.inc\n1,\"3\"oops",
            "reset,in.inc\n1,\"3\"\"4\"",
            "reset,in.inc\n1,3\n\n",
            "reset,in.inc\n1,=1+2",
            "cycle,reset,in.inc\n2,1,3",
            "reset,in.inc\n",
        ] {
            assert!(
                Testbench::import_csv(&doc, csv).is_err(),
                "accepted {csv:?}"
            );
        }
        assert!(Testbench::import_csv(&doc, &"x".repeat(MAX_CSV_BYTES + 1)).is_err());
        let csv = format!("reset,in.inc\n{}", "1,3\n".repeat(257));
        assert!(Testbench::import_csv(&doc, &csv).is_err());
    }
    #[test]
    fn combinational_vectors_preserve_exact_u64_and_optional_assertions() {
        let mut doc = Document::demo();
        for n in &mut doc.nodes {
            if n.width == 32 {
                n.width = 64;
            }
        }
        let rows = Testbench::import_csv(&doc,"reset,in.a,in.b,in.op,expect.result\n0,18446744073709551615,0,0,0xffffffffffffffff\n0,18446744073709551615,1,0,0\n0,1,2,0,\n").unwrap();
        let (_, samples, checks) = Testbench { version: 0, rows }.run(&doc).unwrap();
        assert_eq!((checks.total, checks.passed), (2, 2));
        assert_eq!(samples[0].outputs["result"], u64::MAX.to_string());
        assert_eq!(samples[2].outputs["result"], "3");
    }
    #[test]
    fn guarded_updates_preserve_history_and_legacy_workspaces() {
        let mut w = Workspace::default();
        let document = w.document.clone();
        let source = "reset,in.a,in.b,in.op,expect.result\n0,1,2,0,3\n";
        w.dispatch(
            "testbench.import_csv",
            json!({"expected_revision":0,"expected_version":0,"source":source}),
            "test",
        )
        .unwrap();
        let before = serde_json::to_value(&w).unwrap();
        for args in [
            json!({"expected_revision":0,"expected_version":0,"source":source}),
            json!({"expected_revision":1,"expected_version":1,"source":source}),
            json!({"expected_revision":0,"expected_version":1,"source":"bad"}),
        ] {
            assert!(w.dispatch("testbench.import_csv", args, "test").is_err());
            assert_eq!(serde_json::to_value(&w).unwrap(), before);
        }
        assert!(std::sync::Arc::ptr_eq(&document, &w.document));
        assert!(w.history.is_empty());
        assert!(w.undo.is_empty());
        assert_eq!(w.revision, 0);
        let mut legacy = before;
        legacy.as_object_mut().unwrap().remove("testbench");
        assert!(serde_json::from_value::<Workspace>(legacy)
            .unwrap()
            .testbench
            .rows
            .is_empty());
    }
}
