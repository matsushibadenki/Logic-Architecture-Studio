//! CPU-only breakdown of committed placement edits on the shared large fixture.
#[path = "support/large_graph.rs"]
mod large_graph;
use std::time::Instant;
use unge_core::*;
use unge_render::{SceneChanges, SceneIndex};

fn stats(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    serde_json::json!({"median_ms": values[values.len()/2],
        "p95_ms":values[(values.len()*95).div_ceil(100)-1]})
}
fn main() {
    let document = large_graph::document();
    let mut cases = Vec::new();
    for count in [1, 100] {
        let mut editor = Editor::new(document.clone(), 64).unwrap();
        let mut index = SceneIndex::new(editor.document());
        let mut raw = Vec::new();
        for sample in 0..35 {
            let commands = (1..=count)
                .map(|id| {
                    let id = Id::from_u128(id);
                    let rect = document.placement()[&id];
                    Command::MoveNode {
                        id,
                        rect: Rect {
                            x: rect.x + 96.,
                            y: rect.y + 36.,
                            ..rect
                        },
                    }
                })
                .collect();
            // Pointer release produces a Batch even for a single selected node.
            let command = Command::Batch { commands };
            let changes = SceneChanges::from_command(&command);
            let (_, timing) = editor.execute_timed(command).unwrap();
            let start = Instant::now();
            index.update(editor.document(), &changes);
            let index_ms = start.elapsed().as_secs_f64() * 1000.;
            let inverse = SceneChanges::from_command(editor.undo_command().unwrap());
            let start = Instant::now();
            assert!(editor.undo().unwrap());
            let undo_ms = start.elapsed().as_secs_f64() * 1000.;
            index.update(editor.document(), &inverse);
            assert_eq!(editor.document(), &document);
            // Full validation is an independent oracle outside the measurement.
            editor.document().validate().unwrap();
            if sample >= 5 {
                raw.push((timing, index_ms, undo_ms));
            }
        }
        cases.push(serde_json::json!({
            "moved_nodes":count,"apply":stats(raw.iter().map(|r|r.0.apply_ms).collect()),
            "core_validation":stats(raw.iter().map(|r|r.0.core_validation_ms).collect()),
            "host_validation":stats(raw.iter().map(|r|r.0.host_validation_ms).collect()),
            "history":stats(raw.iter().map(|r|r.0.history_ms).collect()),
            "execute":stats(raw.iter().map(|r|r.0.total_ms).collect()),
            "index":stats(raw.iter().map(|r|r.1).collect()),
            "undo":stats(raw.iter().map(|r|r.2).collect()),"raw":raw,
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "version":1,"scope":"CPU committed move and undo; no host validator, rendering or IPC",
            "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
            "nodes":10000,"edges":30000,"warmup":5,"samples":30,"cases":cases,
        }))
        .unwrap()
    );
}
