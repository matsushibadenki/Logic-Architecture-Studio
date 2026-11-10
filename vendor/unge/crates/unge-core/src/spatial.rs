use crate::{Document, Id, Rect};
use std::collections::{BTreeMap, BTreeSet};

const MAX_REFIT_IDS: usize = 4096;

/// Balanced BVH with bounded rectangle updates; query during pan/zoom.
#[derive(Debug)]
enum Branch {
    Leaf {
        bounds: Rect,
        id: Id,
    },
    Split {
        bounds: Rect,
        left: Box<Branch>,
        right: Box<Branch>,
    },
}
impl Branch {
    fn bounds(&self) -> Rect {
        match self {
            Self::Leaf { bounds, .. } | Self::Split { bounds, .. } => *bounds,
        }
    }
    fn build(items: &mut [(Id, Rect)], depth: usize) -> Self {
        if items.len() == 1 {
            return Self::Leaf {
                id: items[0].0,
                bounds: items[0].1,
            };
        }
        // Only the median partition is needed. Sorting every subtree repeats
        // ordering work; selection keeps balanced partitions with linear work
        // per level. Query results are still sorted by ID at the API boundary.
        items.select_nth_unstable_by(items.len() / 2, |a, b| {
            if depth.is_multiple_of(2) {
                a.1.x.total_cmp(&b.1.x)
            } else {
                a.1.y.total_cmp(&b.1.y)
            }
        });
        let (a, b) = items.split_at_mut(items.len() / 2);
        let left = Box::new(Self::build(a, depth + 1));
        let right = Box::new(Self::build(b, depth + 1));
        let (a, b) = (left.bounds(), right.bounds());
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let bounds = Rect {
            x,
            y,
            width: (a.x + a.width).max(b.x + b.width) - x,
            height: (a.y + a.height).max(b.y + b.height) - y,
        };
        Self::Split {
            bounds,
            left,
            right,
        }
    }
    fn query(&self, area: Rect, output: &mut Vec<Id>) {
        if !self.bounds().intersects(area) {
            return;
        }
        match self {
            Self::Leaf { id, .. } => output.push(*id),
            Self::Split { left, right, .. } => {
                left.query(area, output);
                right.query(area, output);
            }
        }
    }
    fn remove_known_ids(&self, remaining: &mut BTreeSet<Id>) {
        if remaining.is_empty() {
            return;
        }
        match self {
            Self::Leaf { id, .. } => {
                remaining.remove(id);
            }
            Self::Split { left, right, .. } => {
                left.remove_known_ids(remaining);
                right.remove_known_ids(remaining);
            }
        }
    }
    fn refit(&mut self, changes: &BTreeMap<Id, Rect>) -> bool {
        match self {
            Self::Leaf { bounds, id } => {
                if let Some(rect) = changes.get(id) {
                    *bounds = *rect;
                    true
                } else {
                    false
                }
            }
            Self::Split {
                bounds,
                left,
                right,
            } => {
                let a_changed = left.refit(changes);
                let b_changed = right.refit(changes);
                if a_changed || b_changed {
                    let (a, b) = (left.bounds(), right.bounds());
                    let x = a.x.min(b.x);
                    let y = a.y.min(b.y);
                    *bounds = Rect {
                        x,
                        y,
                        width: (a.x + a.width).max(b.x + b.width) - x,
                        height: (a.y + a.height).max(b.y + b.height) - y,
                    };
                }
                a_changed || b_changed
            }
        }
    }
}
#[derive(Debug, Default)]
pub struct SpatialIndex {
    root: Option<Branch>,
    updates: BTreeMap<Id, Option<Rect>>,
    refitted: BTreeSet<Id>,
}
impl SpatialIndex {
    pub fn new(doc: &Document) -> Self {
        let items = doc
            .graph()
            .nodes()
            .keys()
            .map(|id| (*id, doc.placement().get(id).copied().unwrap_or_default()))
            .collect();
        Self::from_rects(items)
    }
    pub fn from_rects(mut items: Vec<(Id, Rect)>) -> Self {
        items.retain(|(_, rect)| rect.valid());
        Self {
            root: (!items.is_empty()).then(|| Branch::build(&mut items, 0)),
            updates: BTreeMap::new(),
            refitted: BTreeSet::new(),
        }
    }
    /// Bounded rectangle upserts over an immutable BVH. False leaves the index
    /// unchanged: rebuild from authoritative geometry when the overlay fills.
    /// Repeated updates to one ID replace its previous rectangle.
    pub fn update_rects(&mut self, changes: &BTreeMap<Id, Rect>) -> bool {
        if changes.len() > 128 {
            return false;
        }
        self.update_entries(
            &changes
                .iter()
                .map(|(id, rect)| (*id, Some(*rect)))
                .collect(),
        )
    }
    /// Atomic bounded upserts/removals. None masks an ID in the base BVH.
    /// Deletions consume the same 128-ID budget as upserts, including absent IDs.
    pub fn update_entries(&mut self, changes: &BTreeMap<Id, Option<Rect>>) -> bool {
        if changes.len() > 128
            || changes.values().flatten().any(|rect| !rect.valid())
            || self.updates.len()
                + changes
                    .keys()
                    .filter(|id| !self.updates.contains_key(id))
                    .count()
                > 128
        {
            return false;
        }
        self.updates
            .extend(changes.iter().map(|(id, rect)| (*id, *rect)));
        true
    }
    /// Update existing base leaves and ancestor bounds without repartitioning.
    /// At most 4096 distinct base IDs may be refitted until reconstruction.
    /// Unknown IDs, invalid rectangles or capacity overflow return false without
    /// mutation. A successful refit replaces any overlay entry for those IDs;
    /// unrelated upserts/deletion markers remain. Query ordering is unchanged.
    /// This traverses the retained tree; use the small overlay for sparse edits.
    pub fn refit_rects(&mut self, changes: &BTreeMap<Id, Rect>) -> bool {
        if changes.is_empty() {
            return true;
        }
        if changes.len() > MAX_REFIT_IDS
            || changes.values().any(|rect| !rect.valid())
            || self.refitted.len()
                + changes
                    .keys()
                    .filter(|id| !self.refitted.contains(id))
                    .count()
                > MAX_REFIT_IDS
        {
            return false;
        }
        let Some(root) = &mut self.root else {
            return false;
        };
        let mut remaining = changes.keys().copied().collect();
        root.remove_known_ids(&mut remaining);
        if !remaining.is_empty() {
            return false;
        }
        root.refit(changes);
        for id in changes.keys() {
            self.updates.remove(id);
            self.refitted.insert(*id);
        }
        true
    }
    pub fn query(&self, area: Rect) -> Vec<Id> {
        let mut result = Vec::new();
        if area.valid()
            && let Some(root) = &self.root
        {
            root.query(area, &mut result);
        }
        if area.valid() && !self.updates.is_empty() {
            result.retain(|id| !self.updates.contains_key(id));
            result.extend(
                self.updates
                    .iter()
                    .filter(|(_, rect)| rect.is_some_and(|r| r.intersects(area)))
                    .map(|(id, _)| *id),
            );
        }
        result.sort();
        result
    }
    pub fn hit_test(&self, point: [f32; 2]) -> Option<Id> {
        self.query(Rect {
            x: point[0],
            y: point[1],
            width: 0.001,
            height: 0.001,
        })
        .last()
        .copied()
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Viewport {
    pub origin: [f32; 2],
    pub zoom: f32,
    pub size: [f32; 2],
}
impl Viewport {
    pub fn validate(self) -> crate::Result<()> {
        if self
            .origin
            .iter()
            .chain(self.size.iter())
            .any(|v| !v.is_finite())
            || !self.zoom.is_finite()
            || !(0.02..=16.0).contains(&self.zoom)
            || self.size.iter().any(|v| *v <= 0.0)
            || !self.world_rect().valid()
        {
            return Err(crate::Error::Invalid("invalid viewport".into()));
        }
        Ok(())
    }
    pub fn world_rect(self) -> Rect {
        Rect {
            x: self.origin[0],
            y: self.origin[1],
            width: self.size[0] / self.zoom,
            height: self.size[1] / self.zoom,
        }
    }
    pub fn to_world(self, screen: [f32; 2]) -> [f32; 2] {
        [
            screen[0] / self.zoom + self.origin[0],
            screen[1] / self.zoom + self.origin[1],
        ]
    }
    pub fn pan(&mut self, delta: [f32; 2]) {
        self.origin[0] -= delta[0] / self.zoom;
        self.origin[1] -= delta[1] / self.zoom;
    }
    pub fn zoom_at(&mut self, screen: [f32; 2], factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let before = self.to_world(screen);
        self.zoom = (self.zoom * factor).clamp(0.02, 16.0);
        self.origin = [
            before[0] - screen[0] / self.zoom,
            before[1] - screen[1] / self.zoom,
        ];
    }
}

/// Shared world-space port geometry for rendering and hit testing.
pub fn port_anchor(rect: Rect, index: usize, count: usize, output: bool) -> [f32; 2] {
    [
        rect.x + if output { rect.width } else { 0.0 },
        rect.y + rect.height * ((index as f32 + 1.0) / (count as f32 + 1.0)),
    ]
}
