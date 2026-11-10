//! CPU geometry benchmark. No GPU, window, IPC or execution backend is measured.
use std::{collections::BTreeSet, hint::black_box, time::Instant};
use unge_core::*;
use unge_render::{SceneChanges, SceneIndex, SceneUpdate};
#[path = "support/large_graph.rs"]
mod large_graph;

fn measure(mut operation: impl FnMut(), iterations: usize) -> serde_json::Value {
    operation(); // warm-up, excluded
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        operation();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    serde_json::json!({ "median_ms": samples[iterations / 2],
        "p95_ms": samples[(iterations * 95).div_ceil(100).saturating_sub(1)],
        "min_ms": samples[0], "max_ms": samples[iterations - 1] })
}
// Every timed transition starts from the actual preceding document. Reverse
// updates run outside the timer and keep the overlay bounded to the same IDs.
fn transition(before: &Document, command: Command, iterations: usize) -> serde_json::Value {
    let changes = SceneChanges::from_command(&command);
    let mut editor = Editor::new(before.clone(), 4).unwrap();
    editor.execute(command).unwrap();
    let inverse = SceneChanges::from_command(editor.undo_command().unwrap());
    let after = editor.document();
    let measure_delta = |a: &Document,
                         b: &Document,
                         forward: &SceneChanges,
                         reverse: &SceneChanges| {
        let mut index = SceneIndex::new(a);
        let mut times = Vec::new();
        for i in 0..=iterations {
            let start = Instant::now();
            let update = index.update(b, forward);
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(update, SceneUpdate::Incremental);
            if i > 0 {
                times.push(elapsed);
            }
            if i == 0 || i == iterations {
                let view = Viewport {
                    origin: [0., 0.],
                    zoom: 0.02,
                    size: [1400., 900.],
                };
                let actual = index.scene(view, &BTreeSet::new()).unwrap();
                let expected = SceneIndex::new(b).scene(view, &BTreeSet::new()).unwrap();
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&actual.quads),
                    bytemuck::cast_slice::<_, u8>(&expected.quads)
                );
                assert_eq!(
                    format!("{:?}", actual.labels),
                    format!("{:?}", expected.labels)
                );
            }
            assert_eq!(index.update(a, reverse), SceneUpdate::Incremental);
        }
        times.sort_by(f64::total_cmp);
        serde_json::json!({"median_ms":times[iterations/2],"p95_ms":times[(iterations*95).div_ceil(100)-1]})
    };
    serde_json::json!({
        "forward_delta": measure_delta(before,after,&changes,&inverse),
        "undo_delta": measure_delta(after,before,&inverse,&changes),
        "forward_full_rebuild": measure(|| { black_box(SceneIndex::new(after)); },iterations),
        "undo_full_rebuild": measure(|| { black_box(SceneIndex::new(before)); },iterations),
    })
}
fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|v| v.parse::<usize>().expect("iteration count"))
        .unwrap_or(20);
    assert!(
        (1..=1000).contains(&iterations),
        "iterations must be 1..1000"
    );
    let ids: Vec<_> = (1..=10_000).map(Id::from_u128).collect();
    let start = Instant::now();
    let document = large_graph::document();
    let fixture_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut editor = Editor::new(document, 0).unwrap();
    let doc = editor.document();
    assert_eq!(doc.graph().nodes().len(), 10_000);
    assert_eq!(doc.graph().edges().len(), 30_000);
    let index = SceneIndex::new(doc);
    let selection = BTreeSet::new();
    let view = Viewport {
        origin: [0.0, 0.0],
        zoom: 1.0,
        size: [1400.0, 900.0],
    };
    let overview = Viewport { zoom: 0.02, ..view };
    let normal = index.scene(view, &selection).unwrap();
    let distant = index.scene(overview, &selection).unwrap();
    let node_build = measure(
        || {
            black_box(SpatialIndex::new(doc));
        },
        iterations,
    );
    let scene_build = measure(
        || {
            black_box(SceneIndex::new(doc));
        },
        iterations,
    );
    let node_query = measure(
        || {
            black_box(index.spatial_index().query(view.world_rect()));
        },
        iterations,
    );
    let normal_scene = measure(
        || {
            black_box(index.scene(view, &selection).unwrap());
        },
        iterations,
    );
    let overview_scene = measure(
        || {
            black_box(index.scene(overview, &selection).unwrap());
        },
        iterations,
    );
    let validation = measure(
        || {
            doc.validate().unwrap();
        },
        iterations,
    );
    let mut new_node = doc.graph().nodes()[&ids[0]].clone();
    new_node.id = Id::from_u128(10001);
    let topology = serde_json::json!({
        "add_node": transition(doc,Command::AddNode { node:new_node,rect:Rect::default() },iterations),
        "remove_node": transition(doc,Command::RemoveNode { id:ids[0] },iterations),
        "connect": transition(doc,Command::Connect { edge:Edge { id:Id::from_u128(999999),
            from:Endpoint { node:ids[0],port:"value".into() }, to:Endpoint { node:ids[9999],port:"value".into() },
        } },iterations),
        "disconnect": transition(doc,Command::Disconnect { id:Id::from_u128(100000) },iterations),
    });
    let move_command = Command::MoveNode {
        id: ids[0],
        rect: Rect {
            x: 1.0,
            y: 1.0,
            ..Rect::default()
        },
    };
    let changes = SceneChanges::from_command(&move_command);
    let mut changed = Editor::new(doc.clone(), 0).unwrap();
    changed.execute(move_command).unwrap();
    let changed = changed.document();
    let mut delta_index = SceneIndex::new(doc);
    let delta_update = measure(
        || {
            assert_eq!(
                delta_index.update(changed, &changes),
                SceneUpdate::Incremental
            );
            black_box(&delta_index);
        },
        iterations,
    );
    let full_move_rebuild = measure(
        || {
            black_box(SceneIndex::new(changed));
        },
        iterations,
    );
    let delta_normal_scene = measure(
        || {
            black_box(delta_index.scene(view, &selection).unwrap());
        },
        iterations,
    );
    let delta_overview_scene = measure(
        || {
            black_box(delta_index.scene(overview, &selection).unwrap());
        },
        iterations,
    );
    let move_edit = measure(
        || {
            editor
                .execute(Command::MoveNode {
                    id: ids[0],
                    rect: Rect {
                        x: 1.0,
                        y: 1.0,
                        ..Rect::default()
                    },
                })
                .unwrap();
        },
        iterations,
    );
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "benchmark_version": 3, "topology_index_transitions": topology, "scope": "CPU geometry only; no GPU/frame-rate claim",
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "iterations": iterations, "nodes": 10000, "edges": 30000,
        "fixture_batch_ms": fixture_ms, "document_validation": validation,
        "node_index_build": node_build, "scene_index_build": scene_build,
        "node_query": node_query, "normal_scene": normal_scene, "overview_scene": overview_scene,
        "move_command_validation": move_edit,
        "move_index_delta": delta_update, "move_index_full_rebuild": full_move_rebuild,
        "delta_normal_scene": delta_normal_scene, "delta_overview_scene": delta_overview_scene,
        "normal_counts": { "nodes": normal.visible_nodes, "edges": normal.visible_edges, "quads": normal.quads.len(), "labels": normal.labels.len() },
        "overview_counts": { "nodes": distant.visible_nodes, "edges": distant.visible_edges, "quads": distant.quads.len(), "labels": distant.labels.len() }
    })).unwrap());
}
