//! CPU Scene generation across zoom levels; output fingerprints are untimed.
#[path = "support/large_graph.rs"]
mod large_graph;
use std::{collections::BTreeSet, hint::black_box, time::Instant};
use unge_core::*;
use unge_render::*;
fn main() {
    let doc = large_graph::document();
    let index = SceneIndex::new(&doc);
    let mut cases = Vec::new();
    for zoom in [1., 0.5, 0.2, 0.1, 0.05, 0.02] {
        let view = Viewport {
            origin: [0., 0.],
            zoom,
            size: [1280., 720.],
        };
        let mut raw = Vec::new();
        let mut phase_raw = Vec::new();
        for i in 0..35 {
            let start = Instant::now();
            let scene = index.scene(view, &BTreeSet::new()).unwrap();
            let ms = start.elapsed().as_secs_f64() * 1000.;
            black_box(scene);
            let (_, timing) = index
                .scene_with_theme_timed(
                    view,
                    &BTreeSet::new(),
                    &unge_interaction::Preview::default(),
                    &LabelCatalog::new(),
                    Locale::En,
                    Theme::Dark,
                )
                .unwrap();
            if i >= 5 {
                raw.push(ms);
                phase_raw.push(timing);
            }
        }
        let (scene, phases) = index
            .scene_with_theme_timed(
                view,
                &BTreeSet::new(),
                &unge_interaction::Preview::default(),
                &LabelCatalog::new(),
                Locale::En,
                Theme::Dark,
            )
            .unwrap();
        let mut fingerprint = 0xcbf29ce484222325u64;
        for b in bytemuck::cast_slice::<_, u8>(&scene.quads)
            .iter()
            .copied()
            .chain(format!("{:?}", scene.labels).bytes())
        {
            fingerprint = (fingerprint ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        let mut sorted = raw.clone();
        sorted.sort_by(f64::total_cmp);
        cases.push(serde_json::json!({"zoom":zoom,"median_ms":sorted[15],"p95_ms":sorted[28],
            "visible_nodes":scene.visible_nodes,"visible_edges":scene.visible_edges,"quads":scene.quads.len(),
            "diagnostic_sample":phases,"phase_raw":phase_raw,"fingerprint":format!("{fingerprint:016x}"),"raw_ms":raw}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"nodes":10000,"edges":30000,"warmup":5,"samples":30,
        "scope":"CPU scene only; no GPU/IPC/Registry; static zoom levels","cases":cases})
        )
        .unwrap()
    );
}
