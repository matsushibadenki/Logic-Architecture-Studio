use std::collections::{BTreeMap, BTreeSet};
use unge_core::{Document, Id, Rect, SpatialIndex};
use unge_interaction::Preview;
const PADDING: f32 = 16.0;
const HEADER: f32 = 28.0;
struct RenderGroup {
    label: String,
    nodes: BTreeSet<Id>,
    bounds: Option<Rect>,
}
#[derive(Default)]
pub(crate) struct GroupIndex {
    groups: BTreeMap<Id, RenderGroup>,
    membership: BTreeMap<Id, BTreeSet<Id>>,
    spatial: SpatialIndex,
}
fn bounds(nodes: &BTreeSet<Id>, rect: impl Fn(Id) -> Option<Rect>) -> Option<Rect> {
    let mut rectangles = nodes.iter().filter_map(|id| rect(*id));
    let first = rectangles.next()?;
    let (mut left, mut top, mut right, mut bottom) = (
        first.x,
        first.y,
        first.x + first.width,
        first.y + first.height,
    );
    for r in rectangles {
        left = left.min(r.x);
        top = top.min(r.y);
        right = right.max(r.x + r.width);
        bottom = bottom.max(r.y + r.height);
    }
    let rect = Rect {
        x: left - PADDING,
        y: top - PADDING - HEADER,
        width: right - left + 2.0 * PADDING,
        height: bottom - top + 2.0 * PADDING + HEADER,
    };
    // Derived frames may exceed the engine's coordinate envelope even when
    // individual nodes are valid. Omit those frames instead of invalid GPU data.
    rect.valid().then_some(rect)
}
impl GroupIndex {
    pub(crate) fn new(doc: &Document) -> Self {
        let mut groups = BTreeMap::new();
        let mut membership: BTreeMap<Id, BTreeSet<Id>> = BTreeMap::new();
        let mut rectangles = Vec::new();
        for group in doc.graph().groups().values() {
            let nodes: BTreeSet<_> = group
                .nodes
                .iter()
                .filter(|id| doc.graph().nodes().contains_key(id))
                .copied()
                .collect();
            let bounds = bounds(&nodes, |id| {
                Some(doc.placement().get(&id).copied().unwrap_or_default())
            });
            if let Some(rect) = bounds {
                rectangles.push((group.id, rect));
            }
            for id in &nodes {
                membership.entry(*id).or_default().insert(group.id);
            }
            groups.insert(
                group.id,
                RenderGroup {
                    label: group.label.clone(),
                    nodes,
                    bounds,
                },
            );
        }
        Self {
            groups,
            membership,
            spatial: SpatialIndex::from_rects(rectangles),
        }
    }
    pub(crate) fn visible(
        &self,
        area: Rect,
        preview: &Preview,
        rect: impl Fn(Id) -> Option<Rect>,
    ) -> Vec<(Rect, &str)> {
        let affected: BTreeSet<_> = preview
            .placement
            .keys()
            .flat_map(|id| self.membership.get(id).into_iter().flatten())
            .copied()
            .collect();
        let mut candidates: BTreeSet<_> = self.spatial.query(area).into_iter().collect();
        candidates.extend(&affected);
        candidates
            .into_iter()
            .filter_map(|id| {
                let group = &self.groups[&id];
                let bounds = if affected.contains(&id) {
                    bounds(&group.nodes, |id| {
                        preview.placement.get(&id).copied().or_else(|| rect(id))
                    })
                } else {
                    group.bounds
                }?;
                bounds
                    .intersects(area)
                    .then_some((bounds, group.label.as_str()))
            })
            .collect()
    }
}
