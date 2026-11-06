//! Connectivity by geometry (PLAN §2.5 step 2): union-find over
//! terminals with a snap tolerance.
//!
//! Rules:
//! * every vertex of a wire (a visible `line`/`arrow` outside element
//!   blocks) is a terminal, and all vertices of one wire are joined —
//!   a polyline is one conductor;
//! * terminals closer than `snap` are joined (grid hash, 3×3 cells);
//! * a terminal lying on the *interior* of a wire segment (within
//!   `snap`) joins that wire — the T-junction rule. Two wires that
//!   merely cross have no terminal at the crossing and stay apart; a
//!   `dot` placed there is a terminal on both and joins them.

use std::collections::HashMap;

/// A point in pic inches.
pub(crate) type Pt = (f64, f64);

/// Disjoint-set forest with path halving and union by size.
pub(crate) struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    pub fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    pub fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let (big, small) = if self.size[ra] >= self.size[rb] {
            (ra, rb)
        } else {
            (rb, ra)
        };
        self.parent[small] = big;
        self.size[big] += self.size[small];
    }
}

/// Join terminals at `points` that coincide within `snap`, and
/// terminals lying on the interior of any segment in `segments`
/// (`(a, b, terminal index of the wire)`).
pub(crate) fn join_geometry(
    uf: &mut UnionFind,
    points: &[Pt],
    segments: &[(Pt, Pt, usize)],
    snap: f64,
) {
    // Coincidence: bucket by snap-sized cells, compare against the 3×3
    // neighborhood so pairs straddling a cell edge still meet.
    let cell = |v: f64| (v / snap).floor() as i64; // `as` saturates
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        let (cx, cy) = (cell(x), cell(y));
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                if let Some(bucket) = grid.get(&(cx.saturating_add(dx), cy.saturating_add(dy))) {
                    for &j in bucket {
                        if dist2(points[i], points[j]) <= snap * snap {
                            uf.union(i, j);
                        }
                    }
                }
            }
        }
        grid.entry((cx, cy)).or_default().push(i);
    }
    // T-junctions: a terminal on a segment's interior.
    for (i, &p) in points.iter().enumerate() {
        for &(a, b, wire) in segments {
            if seg_dist2(p, a, b) <= snap * snap {
                uf.union(i, wire);
            }
        }
    }
}

fn dist2(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

/// Squared distance from `p` to the segment `a`–`b`.
fn seg_dist2(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return dist2(p, a);
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
    dist2(p, (a.0 + t * dx, a.1 + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_across_cell_edges() {
        let pts = [(0.0, 0.0), (0.0009, 0.0), (0.5, 0.5), (0.0020, 0.0)];
        let mut uf = UnionFind::new(pts.len());
        join_geometry(&mut uf, &pts, &[], 1e-3);
        assert_eq!(uf.find(0), uf.find(1));
        assert_ne!(uf.find(0), uf.find(2));
        // 0.0011 from point 1 and 0.002 from point 0: no chain.
        assert_ne!(uf.find(0), uf.find(3));
    }

    #[test]
    fn t_junction_joins_wire() {
        // A terminal at (1, 0) touches the segment (0,0)-(2,0) whose
        // owning terminal is index 1.
        let pts = [(1.0, 0.0), (0.0, 0.0)];
        let mut uf = UnionFind::new(2);
        join_geometry(&mut uf, &pts, &[((0.0, 0.0), (2.0, 0.0), 1)], 1e-3);
        assert_eq!(uf.find(0), uf.find(1));
    }
}
