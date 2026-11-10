mod support;
use design_core::{Document, Kind};
use std::collections::BTreeSet;
use studio_renderer::{project, retained::RetainedProjection};
use unge_core::{Locale, Viewport};
use unge_render::{Scene, SceneIndex, SceneUpdate, Theme};

fn equal(a: &Scene, b: &Scene) {
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.quads),
        bytemuck::cast_slice::<_, u8>(&b.quads)
    );
    assert_eq!(a.labels, b.labels);
    assert_eq!(
        (a.visible_nodes, a.visible_edges),
        (b.visible_nodes, b.visible_edges)
    );
}
fn compare(cache: &RetainedProjection, doc: &Document) {
    let (fresh, labels, _) = project(doc).unwrap();
    let index = SceneIndex::new(&fresh);
    for (zoom, locale, theme) in [
        (0.4, Locale::En, Theme::Dark),
        (1., Locale::Ja, Theme::Dark),
        (0.8, Locale::ZhCn, Theme::Light),
    ] {
        let view = Viewport {
            origin: [-30., -30.],
            size: [1600., 1000.],
            zoom,
        };
        let selected = BTreeSet::from([cache.ids[&doc.nodes[0].id]]);
        equal(
            &cache
                .index
                .scene_with_theme(
                    view,
                    &selected,
                    &Default::default(),
                    &cache.labels,
                    locale,
                    theme,
                )
                .unwrap(),
            &index
                .scene_with_theme(view, &selected, &Default::default(), &labels, locale, theme)
                .unwrap(),
        );
    }
}
#[test]
fn delta_matches_full_rebuild_across_edits_import_undo_and_reorder() {
    let base = Document::demo();
    let mut doc = base.clone();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    compare(&cache, &doc);
    let id = cache.ids["add"];
    doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().x += 20.;
    assert_eq!(cache.sync(&doc).unwrap(), SceneUpdate::Incremental);
    compare(&cache, &doc);
    doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().name = "sum".into();
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().kind = Kind::Sub;
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    doc.edges[0].from = "b".into();
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    doc.nodes.reverse();
    doc.edges.reverse();
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    assert_eq!(cache.ids["add"], id);
    doc.nodes.retain(|n| n.id != "add");
    doc.edges.retain(|e| e.from != "add" && e.to != "add");
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    cache.sync(&base).unwrap();
    compare(&cache, &base);
    let mut imported = support::circuit(200);
    cache.sync(&imported).unwrap();
    compare(&cache, &imported);
    for n in &mut imported.nodes {
        n.width = 16;
    }
    cache.sync(&imported).unwrap();
    compare(&cache, &imported);
    cache.sync(&base).unwrap();
    compare(&cache, &base);
}
#[test]
fn large_single_move_regenerates_only_node_and_incident_edges() {
    let mut doc = support::circuit(2000);
    doc.validate(false).unwrap();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    let view = Viewport {
        origin: [-30., -30.],
        size: [1600., 1000.],
        zoom: 0.1,
    };
    let draw = |c: &RetainedProjection| {
        c.index
            .scene_with_labels(
                view,
                &BTreeSet::new(),
                &Default::default(),
                &c.labels,
                Locale::En,
            )
            .unwrap()
    };
    draw(&cache);
    let moved_id = doc.nodes[1000].id.clone();
    doc.nodes.update(&moved_id, |n| n.x += 18.).unwrap();
    assert_eq!(cache.sync(&doc).unwrap(), SceneUpdate::Incremental);
    let scene = draw(&cache);
    assert_eq!(scene.geometry_stats.rebuilt_nodes, 1);
    assert_eq!(scene.geometry_stats.rebuilt_edges, 4);
    assert_eq!(scene.geometry_stats.reused_nodes, 1999);
    let (fresh, labels, _) = project(&doc).unwrap();
    equal(
        &scene,
        &SceneIndex::new(&fresh)
            .scene_with_labels(
                view,
                &BTreeSet::new(),
                &Default::default(),
                &labels,
                Locale::En,
            )
            .unwrap(),
    );
    let idle = draw(&cache);
    assert_eq!(idle.geometry_stats.rebuilt_nodes, 0);
    assert_eq!(idle.geometry_stats.rebuilt_edges, 0);
}
