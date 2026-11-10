//! Retained bounds refitting versus full scene-index reconstruction, CPU only.
#[path = "support/large_graph.rs"]
mod large_graph;
use std::{collections::BTreeSet, time::Instant};
use unge_core::*;
use unge_render::{SceneChanges, SceneIndex, SceneUpdate};
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
fn stats(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    serde_json::json!({"median_ms":values[values.len()/2],"p95_ms":values[(values.len()*95).div_ceil(100)-1]})
}
fn compare(a: &SceneIndex, b: &SceneIndex, doc: &Document) {
    for viewport in views() {
        assert_eq!(
            a.spatial_index().query(viewport.world_rect()),
            b.spatial_index().query(viewport.world_rect())
        );
        let x = a.scene(viewport, &BTreeSet::new()).unwrap();
        let y = b.scene(viewport, &BTreeSet::new()).unwrap();
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&x.quads),
            bytemuck::cast_slice::<_, u8>(&y.quads)
        );
        assert_eq!(format!("{:?}", x.labels), format!("{:?}", y.labels));
        assert_eq!(
            (x.visible_nodes, x.visible_edges),
            (y.visible_nodes, y.visible_edges)
        );
    }
    for rect in doc.placement().values().take(1100) {
        let p = [rect.x + 1., rect.y + 1.];
        assert_eq!(a.spatial_index().hit_test(p), b.spatial_index().hit_test(p));
    }
}
fn views() -> [Viewport; 3] {
    [[0., 0.], [23000., 0.], [0., 12000.]].map(|origin| Viewport {
        origin,
        zoom: 1.,
        size: [1280., 720.],
    })
}
fn main() {
    let document = large_graph::document();
    let mut cases = Vec::new();
    for count in [1, 100, 1000] {
        let mut editor = Editor::new(document.clone(), 64).unwrap();
        let mut index = SceneIndex::new(&document);
        let mut raw = Vec::new();
        for sample in 0..35 {
            let command = Command::Batch {
                commands: (1..=count)
                    .map(|n| {
                        let id = Id::from_u128(n);
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
                    .collect(),
            };
            let changes = SceneChanges::from_command(&command);
            editor.execute(command).unwrap();
            let start = Instant::now();
            let update = index.update(editor.document(), &changes);
            let update_ms = ms(start);
            assert_eq!(update, SceneUpdate::Incremental);
            let start = Instant::now();
            let full = SceneIndex::new(editor.document());
            let rebuild_ms = ms(start);
            let mut query_ms = Vec::new();
            let mut scene_ms = Vec::new();
            for candidate in [&index, &full] {
                let start = Instant::now();
                let queried: Vec<_> = views()
                    .map(|v| candidate.spatial_index().query(v.world_rect()))
                    .into_iter()
                    .collect();
                std::hint::black_box(queried);
                query_ms.push(ms(start));
                let start = Instant::now();
                for view in views() {
                    std::hint::black_box(candidate.scene(view, &BTreeSet::new()).unwrap());
                }
                scene_ms.push(ms(start));
            }
            compare(&index, &full, editor.document());
            let inverse = SceneChanges::from_command(editor.undo_command().unwrap());
            assert!(editor.undo().unwrap());
            let start = Instant::now();
            assert_eq!(
                index.update(editor.document(), &inverse),
                SceneUpdate::Incremental
            );
            let undo_index_ms = ms(start);
            assert_eq!(editor.document(), &document);
            editor.document().validate().unwrap();
            compare(
                &index,
                &SceneIndex::new(editor.document()),
                editor.document(),
            );
            if sample >= 5 {
                raw.push(
                    serde_json::json!({"update_ms":update_ms,"rebuild_ms":rebuild_ms,
                "undo_index_ms":undo_index_ms,"query_ms":query_ms,"scene_ms":scene_ms}),
                );
            }
        }
        let metric = |key: &str| stats(raw.iter().map(|r| r[key].as_f64().unwrap()).collect());
        let paired =
            |key: &str, i: usize| stats(raw.iter().map(|r| r[key][i].as_f64().unwrap()).collect());
        cases.push(serde_json::json!({"moved_nodes":count,"update":metric("update_ms"),"rebuild":metric("rebuild_ms"),
            "undo_index":metric("undo_index_ms"),"query_retained":paired("query_ms",0),"query_rebuilt":paired("query_ms",1),
            "scene_retained":paired("scene_ms",0),"scene_rebuilt":paired("scene_ms",1),"raw":raw}));
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"version":1,
        "scope":"CPU index update versus same-state full reconstruction; 3 near-view queries/scenes; no Registry/GPU/IPC or edit timing",
        "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"nodes":10000,"edges":30000,
        "warmup":5,"samples":30,"cases":cases})).unwrap());
}
