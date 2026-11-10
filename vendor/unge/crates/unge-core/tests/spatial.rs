use unge_core::{Id, Rect, SpatialIndex};

#[test]
fn bvh_queries_match_brute_force_across_dense_and_sparse_geometry() {
    let mut seed = 17_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as u32 as f32 / u32::MAX as f32
    };
    let mut items: Vec<_> = (1..=2048)
        .map(|id| {
            (
                Id::from_u128(id),
                Rect {
                    x: random() * 20000.0 - 10000.0,
                    y: random() * 20000.0 - 10000.0,
                    width: random() * 800.0 + 1.0,
                    height: random() * 800.0 + 1.0,
                },
            )
        })
        .collect();
    // Equal split coordinates, nested bounds and coincident overlapping nodes.
    items.extend((2049..=2112).map(|id| {
        (
            Id::from_u128(id),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 180.0,
                height: 90.0,
            },
        )
    }));
    let forward = SpatialIndex::from_rects(items.clone());
    let mut reversed = items.clone();
    reversed.reverse();
    let backward = SpatialIndex::from_rects(reversed);
    for _ in 0..200 {
        let area = Rect {
            x: random() * 20000.0 - 10000.0,
            y: random() * 20000.0 - 10000.0,
            width: random() * 3000.0 + 1.0,
            height: random() * 3000.0 + 1.0,
        };
        let mut expected: Vec<_> = items
            .iter()
            .filter(|(_, rect)| rect.intersects(area))
            .map(|(id, _)| *id)
            .collect();
        expected.sort();
        assert_eq!(forward.query(area), expected);
        assert_eq!(backward.query(area), expected);
    }
    assert_eq!(forward.hit_test([90.0, 45.0]), Some(Id::from_u128(2112)));
}

#[test]
fn empty_invalid_and_boundary_geometry_preserves_query_contract() {
    let rect = Rect {
        x: -100.0,
        y: -100.0,
        width: 200.0,
        height: 200.0,
    };
    let index = SpatialIndex::from_rects(vec![
        (Id::from_u128(1), rect),
        (
            Id::from_u128(2),
            Rect {
                x: f32::NAN,
                ..rect
            },
        ),
    ]);
    assert_eq!(
        index.query(Rect {
            x: 100.0,
            y: 100.0,
            width: 1.0,
            height: 1.0
        }),
        vec![Id::from_u128(1)]
    );
    assert!(index.query(Rect { width: 0.0, ..rect }).is_empty());
    assert!(SpatialIndex::from_rects(vec![]).query(rect).is_empty());
    assert!(
        SpatialIndex::from_rects(vec![(
            Id::from_u128(1),
            Rect {
                width: -1.0,
                ..rect
            }
        )])
        .query(rect)
        .is_empty()
    );
}

#[test]
fn bounded_updates_replace_old_bounds_and_reject_atomically() {
    use std::collections::BTreeMap;
    let old = Rect::default();
    let moved = Rect { x: 5000.0, ..old };
    let mut index = SpatialIndex::from_rects(vec![(Id::from_u128(1), old)]);
    assert!(index.update_rects(&[(Id::from_u128(1), moved)].into()));
    assert!(index.query(old).is_empty());
    assert_eq!(index.query(moved), vec![Id::from_u128(1)]);
    assert_eq!(index.hit_test([5010.0, 10.0]), Some(Id::from_u128(1)));
    let bad = [
        (Id::from_u128(1), old),
        (Id::from_u128(2), Rect { width: 0.0, ..old }),
    ]
    .into();
    assert!(!index.update_rects(&bad));
    assert_eq!(index.query(moved), vec![Id::from_u128(1)]);
    let full: BTreeMap<_, _> = (1..=128).map(|id| (Id::from_u128(id), moved)).collect();
    assert!(index.update_rects(&full));
    assert!(!index.update_rects(&[(Id::from_u128(129), old)].into()));
    assert_eq!(index.query(moved).len(), 128);
    assert!(index.query(old).is_empty());
    // Replacing an existing ID stays within the overlay budget.
    assert!(index.update_rects(&[(Id::from_u128(1), old)].into()));
    assert_eq!(index.query(old), vec![Id::from_u128(1)]);
}

#[test]
fn removals_reinsertions_and_mixed_overlay_failures_are_atomic() {
    use std::collections::BTreeMap;
    let a = Id::from_u128(1);
    let b = Id::from_u128(2);
    let rect = Rect::default();
    let mut index = SpatialIndex::from_rects(vec![(a, rect)]);
    assert!(index.update_entries(&[(a, None), (b, Some(rect))].into()));
    assert_eq!(index.query(rect), vec![b]);
    assert_eq!(index.hit_test([10., 10.]), Some(b));
    let invalid = [
        (b, None),
        (
            a,
            Some(Rect {
                width: f32::NAN,
                ..rect
            }),
        ),
    ]
    .into();
    assert!(!index.update_entries(&invalid));
    assert_eq!(index.query(rect), vec![b]);
    assert!(index.update_entries(&[(a, Some(rect)), (b, None)].into()));
    assert_eq!(index.query(rect), vec![a]);
    let full: BTreeMap<_, _> = (1..=128).map(|n| (Id::from_u128(n), None)).collect();
    assert!(index.update_entries(&full));
    assert!(index.query(rect).is_empty());
    assert!(!index.update_entries(&[(a, Some(rect)), (Id::from_u128(129), None)].into()));
    assert!(index.query(rect).is_empty());
    assert!(index.update_entries(&[(a, Some(rect))].into()));
    assert_eq!(index.query(rect), vec![a]);
}

#[test]
fn bulk_refit_preserves_overlay_deletions_order_and_atomic_failures() {
    use std::collections::BTreeMap;
    let mut items: BTreeMap<_, _> = (1..=5000)
        .map(|n| {
            (
                Id::from_u128(n),
                Some(Rect {
                    x: (n % 100) as f32 * 12.,
                    y: (n / 100) as f32 * 12.,
                    width: 10.,
                    height: 10.,
                }),
            )
        })
        .collect();
    let mut index =
        SpatialIndex::from_rects(items.iter().map(|(id, r)| (*id, r.unwrap())).collect());
    let extra = Id::from_u128(6000);
    let overlap = Rect {
        x: -40.,
        y: -30.,
        width: 10.,
        height: 10.,
    };
    let overlay = [
        (Id::from_u128(1), Some(overlap)),
        (Id::from_u128(2), None),
        (extra, Some(overlap)),
    ]
    .into();
    assert!(index.update_entries(&overlay));
    items.extend(overlay);
    let mut changes: BTreeMap<_, _> = (1..=300)
        .map(|n| {
            (
                Id::from_u128(n),
                Rect {
                    x: if n % 2 == 0 { -40. } else { 9000. },
                    y: -30.,
                    width: 10.,
                    height: 10.,
                },
            )
        })
        .collect();
    let areas = [
        overlap,
        Rect {
            x: -100.,
            y: -100.,
            width: 11000.,
            height: 1200.,
        },
        Rect {
            x: 8999.,
            y: -40.,
            width: 20.,
            height: 20.,
        },
    ];
    let check = |index: &SpatialIndex, items: &BTreeMap<Id, Option<Rect>>| {
        for area in areas {
            let expected: Vec<_> = items
                .iter()
                .filter(|(_, r)| r.is_some_and(|r| r.intersects(area)))
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(index.query(area), expected);
        }
        assert_eq!(
            index.hit_test([-39., -29.]),
            items
                .iter()
                .filter(|(_, r)| r.is_some_and(|r| r.intersects(Rect {
                    x: -39.,
                    y: -29.,
                    width: 0.001,
                    height: 0.001
                })))
                .map(|(id, _)| *id)
                .next_back()
        );
    };
    assert!(index.refit_rects(&changes));
    items.extend(changes.iter().map(|(id, r)| (*id, Some(*r))));
    check(&index, &items);
    // Overlay-only IDs cannot be treated as base leaves. Failure is atomic.
    changes.insert(extra, overlap);
    assert!(!index.refit_rects(&changes));
    check(&index, &items);
    changes.remove(&extra);
    changes.insert(
        Id::from_u128(10),
        Rect {
            x: f32::NAN,
            ..overlap
        },
    );
    assert!(!index.refit_rects(&changes));
    check(&index, &items);
    let full: BTreeMap<_, _> = (1..=4096).map(|n| (Id::from_u128(n), overlap)).collect();
    assert!(index.refit_rects(&full));
    items.extend(full.iter().map(|(id, r)| (*id, Some(*r))));
    check(&index, &items);
    assert!(index.refit_rects(&[(Id::from_u128(1), overlap)].into()));
    assert!(!index.refit_rects(&[(Id::from_u128(4097), overlap)].into()));
    check(&index, &items);
    assert!(index.update_entries(&[(Id::from_u128(2), None)].into()));
    items.insert(Id::from_u128(2), None);
    check(&index, &items);
    assert!(index.refit_rects(&BTreeMap::new()));
    assert!(!SpatialIndex::default().refit_rects(&[(Id::from_u128(1), overlap)].into()));
}
