//! OSG cross-talk solver (Phase 4 centerpiece).
//!
//! Generalizes the Phase 3 ODE framework into a coupled system over Ontological
//! Soma Graph nodes connected by `cross_talk` edges. Each node carries local
//! dynamics; each edge adds a coupling term to the downstream node's derivative.
//!
//! This implements the spec's flagship example — adipose tissue secretes IGF-1,
//! which promotes breast-tissue proliferation (adipose → IGF-1 → breast-tissue).
//! The topology (`MacroAnatomy`, `SignalingMolecule` node types and the
//! `cross_talk` edge type) is declared in migration
//! `20240101000007_phase4_osg.sql`; this module is the numerical engine that
//! operates over that graph once instances exist.

use crate::solver::{OdeSystem, State, integrate};

/// Local dynamics closure of a single OSG node: `dy/dt = f(t, y)`.
pub type NodeLocal = Box<dyn Fn(f64, &[f64], &mut [f64]) + Send + Sync>;
/// Coupling closure of a `cross_talk` edge: contributes to the downstream
/// node's derivative given `(t, y_from, y_to)`; length = downstream node dim.
pub type EdgeCoupling = Box<dyn Fn(f64, &[f64], &[f64]) -> Vec<f64> + Send + Sync>;

/// A single OSG node with local (uncoupled) dynamics `dy/dt = f(t, y)`.
pub struct OsNode {
    pub id: String,
    pub dim: usize,
    pub local: NodeLocal,
}

/// A directed coupling edge: `coupling(t, y_from, y_to) -> Vec<f64>` contributes
/// to the downstream node's derivative (length = downstream node dim).
pub struct OsEdge {
    pub from: usize,
    pub to: usize,
    pub coupling: EdgeCoupling,
}

/// A coupled multi-node system built from OSG nodes + `cross_talk` edges.
pub struct CrossTalkSystem {
    nodes: Vec<OsNode>,
    edges: Vec<OsEdge>,
    offsets: Vec<usize>,
}

impl CrossTalkSystem {
    pub fn new(nodes: Vec<OsNode>, edges: Vec<OsEdge>) -> Self {
        let mut offsets = Vec::with_capacity(nodes.len());
        let mut acc = 0usize;
        for n in &nodes {
            offsets.push(acc);
            acc += n.dim;
        }
        Self {
            nodes,
            edges,
            offsets,
        }
    }

    pub fn dim(&self) -> usize {
        self.offsets.last().copied().unwrap_or(0) + self.nodes.last().map(|n| n.dim).unwrap_or(0)
    }

    /// Integrate the coupled system with RK4.
    pub fn simulate(&self, t0: f64, y0: &[f64], dt: f64, steps: usize) -> Vec<(f64, State)> {
        integrate(self, t0, y0, dt, steps)
    }

    /// Index of a node by id (useful for inspecting trajectory slices).
    pub fn node_offset(&self, idx: usize) -> usize {
        self.offsets[idx]
    }
}

impl OdeSystem for CrossTalkSystem {
    fn dim(&self) -> usize {
        self.dim()
    }

    fn deriv(&self, t: f64, y: &[f64], dydt: &mut [f64]) {
        // Local dynamics first.
        for (i, node) in self.nodes.iter().enumerate() {
            let start = self.offsets[i];
            let slice = &y[start..start + node.dim];
            let mut local = vec![0.0; node.dim];
            (node.local)(t, slice, &mut local);
            for (d, l) in dydt[start..start + node.dim].iter_mut().zip(local.iter()) {
                *d = *l;
            }
        }
        // Then add edge couplings to the downstream node.
        for edge in &self.edges {
            let fs = self.offsets[edge.from];
            let ts = self.offsets[edge.to];
            let fdim = self.nodes[edge.from].dim;
            let tdim = self.nodes[edge.to].dim;
            let yfrom = &y[fs..fs + fdim];
            let yto = &y[ts..ts + tdim];
            let c = (edge.coupling)(t, yfrom, yto);
            for (d, cj) in dydt[ts..ts + tdim].iter_mut().zip(c.iter()) {
                *d += *cj;
            }
        }
    }
}

/// Build the spec's adipose → IGF-1 → breast-tissue example as a coupled system.
///
/// State layout: `[adipose_activity, igf1, breast_proliferation]`.
/// - Adipose activity is held near a steady secretory level (local decay toward 1).
/// - IGF-1 is cleared over time and driven by adipose secretion (coupling).
/// - Breast proliferation grows logistically under IGF-1 exposure (coupling).
pub fn build_adipose_igf1_breast(
    adipose_secretion: f64,
    igf1_clearance: f64,
    igf1_growth: f64,
) -> (CrossTalkSystem, Vec<f64>) {
    let nodes = vec![
        OsNode {
            id: "AdiposeTissue".to_string(),
            dim: 1,
            local: Box::new(|_, y, d| {
                // relax toward an active secretory steady state of 1.0
                d[0] = (1.0 - y[0]) * 0.1;
            }),
        },
        OsNode {
            id: "IGF1".to_string(),
            dim: 1,
            local: Box::new(move |_, y, d| {
                d[0] = -igf1_clearance * y[0];
            }),
        },
        OsNode {
            id: "BreastTissue".to_string(),
            dim: 1,
            local: Box::new(|_, y, d| {
                // spontaneous decay of proliferation signal
                d[0] = -0.05 * y[0];
            }),
        },
    ];

    let edges = vec![
        // Adipose -> IGF1: secretion proportional to adipose activity.
        OsEdge {
            from: 0,
            to: 1,
            coupling: Box::new(move |_, y_from, _| vec![adipose_secretion * y_from[0]]),
        },
        // IGF1 -> Breast: logistic growth of proliferation under IGF-1 exposure.
        OsEdge {
            from: 1,
            to: 2,
            coupling: Box::new(move |_, y_from, y_to| {
                vec![igf1_growth * y_from[0] * (1.0 - y_to[0].clamp(0.0, 1.0))]
            }),
        },
    ];

    let system = CrossTalkSystem::new(nodes, edges);
    let init = vec![1.0, 0.0, 0.0];
    (system, init)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crosstalk_system_dim_and_offsets() {
        let (sys, init) = build_adipose_igf1_breast(0.5, 0.2, 0.3);
        assert_eq!(sys.dim(), 3);
        assert_eq!(init.len(), 3);
        assert_eq!(sys.node_offset(0), 0);
        assert_eq!(sys.node_offset(1), 1);
        assert_eq!(sys.node_offset(2), 2);
    }

    #[test]
    fn test_igf1_is_produced_from_adipose() {
        let (sys, init) = build_adipose_igf1_breast(0.5, 0.2, 0.3);
        let traj = sys.simulate(0.0, &init, 0.05, 400);
        let (_, last) = traj.last().unwrap();
        // IGF-1 (index 1) should be present because adipose secretes it.
        assert!(last[1] > 0.0, "IGF-1 should be produced, got {}", last[1]);
    }

    #[test]
    fn test_breast_proliferation_requires_igf1_coupling() {
        // With coupling: adipose secretes IGF-1, which drives breast proliferation.
        let (coupled, init) = build_adipose_igf1_breast(0.8, 0.2, 0.5);
        let coupled_traj = coupled.simulate(0.0, &init, 0.05, 600);
        let coupled_breast = coupled_traj.last().unwrap().1[2];

        // Without coupling (no secretion, no growth): breast stays at 0.
        let nodes = vec![
            OsNode {
                id: "AdiposeTissue".to_string(),
                dim: 1,
                local: Box::new(|_, y, d| d[0] = (1.0 - y[0]) * 0.1),
            },
            OsNode {
                id: "IGF1".to_string(),
                dim: 1,
                local: Box::new(|_, y, d| d[0] = -0.2 * y[0]),
            },
            OsNode {
                id: "BreastTissue".to_string(),
                dim: 1,
                local: Box::new(|_, y, d| d[0] = -0.05 * y[0]),
            },
        ];
        let uncoupled = CrossTalkSystem::new(nodes, vec![]);
        let uncoupled_traj = uncoupled.simulate(0.0, &init, 0.05, 600);
        let uncoupled_breast = uncoupled_traj.last().unwrap().1[2];

        assert!(
            coupled_breast > uncoupled_breast,
            "coupled breast proliferation {} should exceed uncoupled {}",
            coupled_breast,
            uncoupled_breast
        );
        assert!(coupled_breast > 0.0);
    }
}
