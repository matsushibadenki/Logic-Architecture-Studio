//! Deterministic orthogonal routing shared by the native renderer and web view.
//! Coordinates and paths are derived presentation data; never mutate the graph.
use serde::Serialize;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
pub type Point = [f32; 2];
pub type Bounds = [f32; 4];
const CLEARANCE: f32 = 12.;
const LEAD: f32 = 18.;
const MAX_GRID: usize = 40_000;
#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub id: u128,
    pub rect: Bounds,
}
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub id: u128,
    pub source: (u128, usize),
    pub target: (u128, usize),
    pub start: Point,
    pub end: Point,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Route {
    pub points: Vec<Point>,
    pub bridges: Vec<Point>,
    /// Overlapping/covered ports or exhausted search: show endpoint warnings,
    /// never a misleading line through an obstacle.
    pub blocked: bool,
}
#[derive(Default)]
struct Index {
    rects: Vec<Bounds>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    large: Vec<usize>,
}
fn cells(r: Bounds) -> Option<(i64, i64, i64, i64)> {
    let x = (r[0] / 256.).floor() as i64;
    let y = (r[1] / 256.).floor() as i64;
    let xx = ((r[0] + r[2]) / 256.).floor() as i64;
    let yy = ((r[1] + r[3]) / 256.).floor() as i64;
    ((xx - x + 1).saturating_mul(yy - y + 1) <= 4096).then_some((x, y, xx, yy))
}
fn intersects(a: Bounds, b: Bounds) -> bool {
    a[0] <= b[0] + b[2] && a[0] + a[2] >= b[0] && a[1] <= b[1] + b[3] && a[1] + a[3] >= b[1]
}
fn grow(r: Bounds, n: f32) -> Bounds {
    [r[0] - n, r[1] - n, r[2] + 2. * n, r[3] + 2. * n]
}
fn bounds(a: Point, b: Point) -> Bounds {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        (a[0] - b[0]).abs(),
        (a[1] - b[1]).abs(),
    ]
}
impl Index {
    fn insert(&mut self, rect: Bounds) {
        let id = self.rects.len();
        self.rects.push(rect);
        if let Some((x, y, xx, yy)) = cells(rect) {
            for i in x..=xx {
                for j in y..=yy {
                    self.cells.entry((i, j)).or_default().push(id);
                }
            }
        } else {
            self.large.push(id);
        }
    }
    fn query(&self, r: Bounds) -> Vec<usize> {
        if let Some((x, y, xx, yy)) = cells(r) {
            let mut ids = Vec::new();
            for i in x..=xx {
                for j in y..=yy {
                    if let Some(indices) = self.cells.get(&(i, j)) {
                        ids.extend_from_slice(indices);
                    }
                }
            }
            if x != xx || y != yy || !self.large.is_empty() {
                ids.extend_from_slice(&self.large);
                ids.sort_unstable();
                ids.dedup();
            }
            ids.into_iter()
                .filter(|&i| intersects(self.rects[i], r))
                .collect()
        } else {
            self.rects
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| intersects(b, r).then_some(i))
                .collect()
        }
    }
}
#[derive(Clone, Copy)]
struct Segment {
    a: Point,
    b: Point,
    net: (u128, usize),
    terminal: bool,
}
pub struct Router {
    obstacles: Vec<Obstacle>,
    nodes: Index,
    wires: Index,
    segments: Vec<Segment>,
}
fn hits(a: Point, b: Point, r: Bounds) -> bool {
    if a[1] == b[1] {
        a[1] > r[1] + 0.01
            && a[1] < r[1] + r[3] - 0.01
            && a[0].min(b[0]) < r[0] + r[2] - 0.01
            && a[0].max(b[0]) > r[0] + 0.01
    } else {
        a[0] > r[0] + 0.01
            && a[0] < r[0] + r[2] - 0.01
            && a[1].min(b[1]) < r[1] + r[3] - 0.01
            && a[1].max(b[1]) > r[1] + 0.01
    }
}
fn cross(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    if (a[1] == b[1]) == (c[1] == d[1]) {
        return None;
    }
    let p = if a[1] == b[1] {
        [c[0], a[1]]
    } else {
        [a[0], c[1]]
    };
    let within = |p: Point, a: Point, b: Point| {
        p[0] >= a[0].min(b[0]) - 0.01
            && p[0] <= a[0].max(b[0]) + 0.01
            && p[1] >= a[1].min(b[1]) - 0.01
            && p[1] <= a[1].max(b[1]) + 0.01
    };
    (within(p, a, b) && within(p, c, d)).then_some(p)
}
fn distance(a: Point, b: Point) -> f32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs()
}
fn simplify(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for p in points {
        if out.last() == Some(&p) {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            if (a[0] == b[0] && b[0] == p[0]) || (a[1] == b[1] && b[1] == p[1]) {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}
impl Router {
    pub fn new(obstacles: Vec<Obstacle>) -> Self {
        let mut nodes = Index::default();
        for o in &obstacles {
            nodes.insert(grow(o.rect, CLEARANCE));
        }
        Self {
            obstacles,
            nodes,
            wires: Index::default(),
            segments: Vec::new(),
        }
    }
    fn clear(&self, a: Point, b: Point, except: Option<u128>) -> bool {
        self.nodes
            .query(bounds(a, b))
            .into_iter()
            .all(|i| Some(self.obstacles[i].id) == except || !hits(a, b, self.nodes.rects[i]))
    }
    fn wire_cost(&self, a: Point, b: Point, net: (u128, usize), crossings: bool) -> Option<f32> {
        let mut cost = 0.;
        for i in self.wires.query(grow(bounds(a, b), CLEARANCE)) {
            let s = self.segments[i];
            if s.net == net {
                continue;
            }
            if (a[1] == b[1]) == (s.a[1] == s.b[1]) {
                let axis = usize::from(a[1] == b[1]);
                let along = 1 - axis;
                if (a[axis] - s.a[axis]).abs() < CLEARANCE - 0.01
                    && a[along].min(b[along]) < s.a[along].max(s.b[along]) + CLEARANCE
                    && a[along].max(b[along]) > s.a[along].min(s.b[along]) - CLEARANCE
                {
                    return None;
                }
            } else if let Some(p) = cross(a, b, s.a, s.b) {
                // Avoid a T-shaped false junction at another net's corner.
                if s.terminal || !crossings || distance(p, s.a) < 8. || distance(p, s.b) < 8. {
                    return None;
                }
                cost += 1200.;
            }
        }
        Some(cost)
    }
    fn path_clear(&self, p: &[Point], net: (u128, usize), crossings: bool) -> bool {
        p.windows(2).all(|s| {
            self.clear(s[0], s[1], None) && self.wire_cost(s[0], s[1], net, crossings).is_some()
        })
    }
    pub fn route_all(mut self, links: &[Link]) -> BTreeMap<u128, Route> {
        // Reserve every terminal before routing any net. An early wire must
        // not consume the short escape corridor of a later port.
        let mut terminals = HashSet::new();
        for l in links {
            for (a, b) in [
                (l.start, [l.start[0] + LEAD, l.start[1]]),
                ([l.end[0] - LEAD, l.end[1]], l.end),
            ] {
                // A shared output contributes one identical reserved lead.
                // Terminal constraints are Boolean, so multiplicity has no effect.
                let key = (l.source, a.map(f32::to_bits), b.map(f32::to_bits));
                if !terminals.insert(key) {
                    continue;
                }
                self.wires.insert(bounds(a, b));
                self.segments.push(Segment {
                    a,
                    b,
                    net: l.source,
                    terminal: true,
                });
            }
        }
        // Geometric order keeps output independent of map insertion/history order.
        let mut links = links.to_vec();
        links.sort_by(|a, b| {
            a.start[0]
                .total_cmp(&b.start[0])
                .then(a.start[1].total_cmp(&b.start[1]))
                .then(a.end[0].total_cmp(&b.end[0]))
                .then(a.end[1].total_cmp(&b.end[1]))
                .then(a.id.cmp(&b.id))
        });
        links.iter().map(|l| (l.id, self.route(*l))).collect()
    }
    fn route(&mut self, l: Link) -> Route {
        let a = [l.start[0] + LEAD, l.start[1]];
        let b = [l.end[0] - LEAD, l.end[1]];
        if !self.clear(l.start, a, Some(l.source.0)) || !self.clear(b, l.end, Some(l.target.0)) {
            return Route {
                blocked: true,
                ..Default::default()
            };
        }
        let mut result = None;
        let mid = (a[0] + b[0]) * 0.5;
        // Cheap common case before building a visibility graph.
        let direct = vec![a, [mid, a[1]], [mid, b[1]], b];
        if self.path_clear(&direct, l.source, false)
            && self.wire_cost(l.start, a, l.source, false).is_some()
            && self.wire_cost(b, l.end, l.source, false).is_some()
        {
            result = Some(direct);
        }
        for crossings in [false, true] {
            if result.is_some() {
                break;
            }
            if self.wire_cost(l.start, a, l.source, crossings).is_none()
                || self.wire_cost(b, l.end, l.source, crossings).is_none()
            {
                continue;
            }
            for margin in [96., 384., 1536.] {
                if let Some(p) = self.search(a, b, l.source, grow(bounds(a, b), margin), crossings)
                {
                    result = Some(p);
                    break;
                }
            }
        }
        let Some(mut path) = result else {
            return Route {
                blocked: true,
                ..Default::default()
            };
        };
        path.insert(0, l.start);
        path.push(l.end);
        let points = simplify(path);
        let mut bridges = Vec::new();
        for s in points.windows(2) {
            for i in self.wires.query(bounds(s[0], s[1])) {
                let old = self.segments[i];
                if old.net != l.source {
                    if let Some(p) = cross(s[0], s[1], old.a, old.b) {
                        if !bridges.contains(&p) {
                            bridges.push(p);
                        }
                    }
                }
            }
        }
        for s in points.windows(2) {
            self.wires.insert(bounds(s[0], s[1]));
            self.segments.push(Segment {
                a: s[0],
                b: s[1],
                net: l.source,
                terminal: false,
            });
        }
        Route {
            points,
            bridges,
            blocked: false,
        }
    }
    fn search(
        &self,
        a: Point,
        b: Point,
        net: (u128, usize),
        region: Bounds,
        crossings: bool,
    ) -> Option<Vec<Point>> {
        let mut xs = vec![a[0], b[0], region[0], region[0] + region[2]];
        let mut ys = vec![a[1], b[1], region[1], region[1] + region[3]];
        for i in self.nodes.query(region) {
            let r = self.nodes.rects[i];
            xs.extend([r[0], r[0] + r[2]]);
            ys.extend([r[1], r[1] + r[3]]);
        }
        for i in self.wires.query(region) {
            let s = self.segments[i];
            if s.net == net {
                continue;
            }
            for p in [s.a, s.b] {
                xs.extend([p[0] - CLEARANCE, p[0] + CLEARANCE]);
                ys.extend([p[1] - CLEARANCE, p[1] + CLEARANCE]);
            }
        }
        let clean = |v: &mut Vec<f32>, lo: f32, hi: f32| {
            v.retain(|x| *x >= lo && *x <= hi);
            v.sort_by(f32::total_cmp);
            v.dedup();
        };
        clean(&mut xs, region[0], region[0] + region[2]);
        clean(&mut ys, region[1], region[1] + region[3]);
        let w = xs.len();
        let h = ys.len();
        if w * h > MAX_GRID {
            return None;
        }
        let point = |i: usize| [xs[i % w], ys[i / w]];
        let start = ys.binary_search_by(|v| v.total_cmp(&a[1])).ok()? * w
            + xs.binary_search_by(|v| v.total_cmp(&a[0])).ok()?;
        let goal = ys.binary_search_by(|v| v.total_cmp(&b[1])).ok()? * w
            + xs.binary_search_by(|v| v.total_cmp(&b[0])).ok()?;
        let mut dist = vec![f32::INFINITY; w * h * 2];
        let mut prev = vec![usize::MAX; w * h * 2];
        let mut heap = BinaryHeap::new();
        for dir in 0..2 {
            dist[start * 2 + dir] = 0.;
            heap.push(Visit {
                estimate: distance(a, b),
                cost: 0.,
                state: start * 2 + dir,
            });
        }
        let mut checks: HashMap<(usize, usize), Option<f32>> = HashMap::new();
        while let Some(v) = heap.pop() {
            if v.cost > dist[v.state] {
                continue;
            }
            let i = v.state / 2;
            if i == goal {
                let mut path = vec![point(i)];
                let mut state = v.state;
                while prev[state] != usize::MAX {
                    state = prev[state];
                    path.push(point(state / 2));
                }
                path.reverse();
                return Some(simplify(path));
            }
            let x = i % w;
            let y = i / w;
            for (j, dir) in [
                (x > 0, i.wrapping_sub(1), 0),
                (x + 1 < w, i + 1, 0),
                (y > 0, i.wrapping_sub(w), 1),
                (y + 1 < h, i + w, 1),
            ]
            .into_iter()
            .filter_map(|(ok, j, d)| ok.then_some((j, d)))
            {
                let edge = *checks.entry((i.min(j), i.max(j))).or_insert_with(|| {
                    let p = point(i);
                    let q = point(j);
                    if !self.clear(p, q, None) {
                        None
                    } else {
                        self.wire_cost(p, q, net, crossings)
                            .map(|c| c + distance(p, q))
                    }
                });
                if let Some(c) = edge {
                    let cost = v.cost + c + if dir != v.state % 2 { 24. } else { 0. };
                    let state = j * 2 + dir;
                    if cost < dist[state] {
                        dist[state] = cost;
                        prev[state] = v.state;
                        heap.push(Visit {
                            estimate: cost + distance(point(j), b),
                            cost,
                            state,
                        });
                    }
                }
            }
        }
        None
    }
}
#[derive(Clone, Copy)]
struct Visit {
    estimate: f32,
    cost: f32,
    state: usize,
}
impl PartialEq for Visit {
    fn eq(&self, o: &Self) -> bool {
        self.estimate == o.estimate && self.state == o.state
    }
}
impl Eq for Visit {}
impl Ord for Visit {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.estimate
            .total_cmp(&self.estimate)
            .then(o.state.cmp(&self.state))
    }
}
impl PartialOrd for Visit {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Route {
    /// Small overpass arcs distinguish unavoidable crossings from junctions.
    pub fn display_points(&self) -> Vec<Point> {
        let mut out = Vec::new();
        for segment in self.points.windows(2) {
            let a = segment[0];
            let b = segment[1];
            if out.is_empty() {
                out.push(a);
            }
            let length = distance(a, b);
            let along = [
                (b[0] - a[0]) / length.max(0.001),
                (b[1] - a[1]) / length.max(0.001),
            ];
            let normal = [-along[1], along[0]];
            let mut bridges: Vec<_> = self
                .bridges
                .iter()
                .copied()
                .filter(|p| distance(a, *p) + distance(*p, b) < length + 0.01)
                .collect();
            bridges.sort_by(|p, q| distance(a, *p).total_cmp(&distance(a, *q)));
            for p in bridges {
                let radius = 6.0_f32.min(distance(a, p)).min(distance(p, b));
                if radius < 0.1 {
                    continue;
                }
                for i in 0..=8 {
                    let t = std::f32::consts::PI * (i as f32 / 8.);
                    out.push([
                        p[0] - along[0] * radius * t.cos() + normal[0] * radius * t.sin(),
                        p[1] - along[1] * radius * t.cos() + normal[1] * radius * t.sin(),
                    ]);
                }
            }
            out.push(b);
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: u128, x: f32, y: f32) -> Obstacle {
        Obstacle {
            id,
            rect: [x, y, 288., 288.],
        }
    }
    fn link(id: u128, source: u128, target: u128, start: Point, end: Point) -> Link {
        Link {
            id,
            source: (source, 0),
            target: (target, 0),
            start,
            end,
        }
    }
    fn assert_clear(route: &Route, obstacles: &[Obstacle], l: Link) {
        assert!(!route.blocked, "route unexpectedly blocked: {l:?}");
        assert_eq!(route.points.first(), Some(&l.start));
        assert_eq!(route.points.last(), Some(&l.end));
        for s in route.points.windows(2) {
            assert!(s[0][0] == s[1][0] || s[0][1] == s[1][1]);
            for o in obstacles {
                assert!(
                    !hits(s[0], s[1], o.rect),
                    "wire enters node {}: {s:?}",
                    o.id
                );
            }
        }
    }
    #[test]
    fn avoids_unconnected_node_and_tracks_its_movement() {
        let l = link(10, 1, 2, [288., 144.], [1000., 144.]);
        let mut nodes = vec![node(1, 0., 0.), node(2, 1000., 0.), node(3, 500., 500.)];
        let straight = Router::new(nodes.clone()).route_all(&[l]);
        assert_eq!(straight[&10].points, vec![l.start, l.end]);
        nodes[2].rect[1] = 0.;
        let detour = Router::new(nodes.clone()).route_all(&[l]);
        assert_clear(&detour[&10], &nodes, l);
        assert!(detour[&10].points.len() > 2);
        nodes[2].rect[1] = 500.;
        assert_eq!(Router::new(nodes).route_all(&[l]), straight);
    }
    #[test]
    fn multiple_feedback_nets_do_not_share_segments() {
        let nodes = vec![
            node(1, 990., 0.),
            node(2, 330., 340.),
            node(3, 0., 340.),
            node(4, 660., 340.),
        ];
        let a = link(10, 1, 3, [1278., 144.], [0., 436.]);
        let b = Link {
            target: (3, 1),
            ..link(11, 2, 3, [618., 484.], [0., 532.])
        };
        let routes = Router::new(nodes.clone()).route_all(&[a, b]);
        assert_clear(&routes[&10], &nodes, a);
        assert_clear(&routes[&11], &nodes, b);
        for s in routes[&10].points.windows(2) {
            for t in routes[&11].points.windows(2) {
                if (s[0][1] == s[1][1]) == (t[0][1] == t[1][1]) {
                    let axis = usize::from(s[0][1] == s[1][1]);
                    let along = 1 - axis;
                    assert!(
                        s[0][axis] != t[0][axis]
                            || s[0][along].min(s[1][along]) >= t[0][along].max(t[1][along])
                            || t[0][along].min(t[1][along]) >= s[0][along].max(s[1][along])
                    );
                }
            }
        }
        assert_eq!(routes, Router::new(nodes).route_all(&[b, a]));
    }
    #[test]
    fn closed_ports_report_blocked_instead_of_drawing_through_a_node() {
        let l = link(10, 1, 2, [288., 144.], [600., 144.]);
        let routes = Router::new(vec![node(1, 0., 0.), node(2, 600., 0.), node(3, 280., 0.)])
            .route_all(&[l]);
        assert!(routes[&10].blocked);
        assert!(routes[&10].points.is_empty());
    }
    #[test]
    fn unavoidable_crossing_gets_an_overpass() {
        // Four terminals alternate around a ring; two different nets must cross.
        let nodes = vec![
            node(1, 0., 400.),
            node(2, 1600., 400.),
            node(3, 800., 0.),
            node(4, 800., 800.),
        ];
        let links = [
            link(10, 1, 2, [288., 544.], [1600., 544.]),
            link(11, 3, 4, [1088., 144.], [800., 944.]),
        ];
        let routes = Router::new(nodes.clone()).route_all(&links);
        for l in links {
            assert_clear(&routes[&l.id], &nodes, l);
        }
        // Open plane may route around the first net; either outcome is valid.
        for r in routes.values() {
            if !r.bridges.is_empty() {
                assert!(r.display_points().len() > r.points.len());
            }
        }
        let r = Route {
            points: vec![[0., 50.], [100., 50.]],
            bridges: vec![[50., 50.]],
            blocked: false,
        };
        let p = r.display_points();
        assert!(p.iter().any(|p| p[1] > 55.));
        assert!(!p
            .windows(2)
            .any(|s| s[0][1] == 50. && s[1][1] == 50. && s[0][0] < 50. && s[1][0] > 50.));
    }
    #[test]
    fn nonplanar_nets_use_bridges_without_collinear_sharing() {
        let nodes: Vec<_> = (0..3)
            .flat_map(|i| {
                [
                    node(i + 1, 0., i as f32 * 400.),
                    node(i + 4, 1400., i as f32 * 400.),
                ]
            })
            .collect();
        let links: Vec<_> = (0..3)
            .flat_map(|i| {
                (0..3).map(move |j| Link {
                    id: 10 + i * 3 + j,
                    source: (i + 1, 0),
                    target: (j + 4, i as usize),
                    start: [288., i as f32 * 400. + 144.],
                    end: [1400., j as f32 * 400. + 72. * (i + 1) as f32],
                })
            })
            .collect();
        let routes = Router::new(nodes.clone()).route_all(&links);
        for l in &links {
            assert_clear(&routes[&l.id], &nodes, *l);
        }
        assert!(routes.values().any(|r| !r.bridges.is_empty()));
        for r in routes.values() {
            for p in &r.bridges {
                assert!(
                    r.points.windows(2).any(|s| distance(s[0], *p) > 0.1
                        && distance(s[1], *p) > 0.1
                        && (distance(s[0], *p) + distance(*p, s[1]) - distance(s[0], s[1])).abs()
                            < 0.01),
                    "bridge at an undrawable corner: {r:?}"
                );
            }
        }
    }

    #[test]
    fn two_thousand_nodes_routing_budget() {
        let nodes: Vec<_> = (0..2000)
            .map(|i| node(i + 1, (i % 40) as f32 * 400., (i / 40) as f32 * 400.))
            .collect();
        let links: Vec<_> = (0..2000)
            .filter(|i| i % 40 != 39)
            .map(|i| {
                link(
                    10000 + i,
                    i + 1,
                    i + 2,
                    [
                        nodes[i as usize].rect[0] + 288.,
                        nodes[i as usize].rect[1] + 144.,
                    ],
                    [
                        nodes[i as usize + 1].rect[0],
                        nodes[i as usize + 1].rect[1] + 144.,
                    ],
                )
            })
            .collect();
        let started = std::time::Instant::now();
        let routes = Router::new(nodes).route_all(&links);
        assert!(routes.values().all(|r| !r.blocked));
        eprintln!(
            "2,000 nodes / {} wires: {:?}",
            links.len(),
            started.elapsed()
        );
        assert!(started.elapsed().as_secs() < 5);
    }
}
