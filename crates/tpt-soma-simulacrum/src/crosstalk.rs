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
//! `20240101000007_phase4_osg.sql`. `build_from_topology` turns rows of that
//! graph into a runnable system, so a systemic query can be answered from the
//! stored topology rather than from a hand-built model.

use std::collections::HashMap;

use crate::solver::{OdeSystem, State, integrate};
use crate::{Result, SimulacrumError};

/// Plexus edge type that carries cross-talk couplings between OSG nodes.
pub const CROSS_TALK_EDGE_TYPE: &str = "cross_talk";

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

/// OSG node kinds the topology builder knows how to instantiate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsKind {
    /// `MacroAnatomy`: adipose tissue, a secretory source.
    AdiposeTissue,
    /// `SignalingMolecule`: IGF-1.
    Igf1,
    /// `MacroAnatomy`: breast tissue, a proliferative target.
    BreastTissue,
}

impl OsKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "AdiposeTissue" => Ok(Self::AdiposeTissue),
            "IGF1" => Ok(Self::Igf1),
            "BreastTissue" => Ok(Self::BreastTissue),
            other => Err(SimulacrumError::InvalidInput(format!(
                "unsupported OSG node kind: {other}"
            ))),
        }
    }
}

/// Parameters of the adipose → IGF-1 → breast-tissue dynamics.
#[derive(Debug, Clone, Copy)]
pub struct CrossTalkParams {
    /// IGF-1 secretion per unit adipose activity.
    pub adipose_secretion: f64,
    /// First-order IGF-1 clearance rate.
    pub igf1_clearance: f64,
    /// Logistic proliferation rate under IGF-1 exposure.
    pub igf1_growth: f64,
}

impl Default for CrossTalkParams {
    fn default() -> Self {
        Self {
            adipose_secretion: 0.5,
            igf1_clearance: 0.2,
            igf1_growth: 0.3,
        }
    }
}

/// One OSG node row: its id and OSG kind label.
#[derive(Debug, Clone)]
pub struct OsNodeRow {
    pub id: String,
    pub kind: String,
}

/// One `cross_talk` edge row, referencing node ids.
#[derive(Debug, Clone)]
pub struct OsEdgeRow {
    pub from: String,
    pub to: String,
    pub edge_type: String,
}

/// Topology pulled from the OSG (Plexus nodes and edges).
#[derive(Debug, Clone, Default)]
pub struct OsTopology {
    pub nodes: Vec<OsNodeRow>,
    pub edges: Vec<OsEdgeRow>,
}

fn local_dynamics(kind: OsKind, p: &CrossTalkParams) -> NodeLocal {
    let clearance = p.igf1_clearance;
    match kind {
        // relax toward an active secretory steady state of 1.0
        OsKind::AdiposeTissue => Box::new(|_, y, d| d[0] = (1.0 - y[0]) * 0.1),
        OsKind::Igf1 => Box::new(move |_, y, d| d[0] = -clearance * y[0]),
        // spontaneous decay of proliferation signal
        OsKind::BreastTissue => Box::new(|_, y, d| d[0] = -0.05 * y[0]),
    }
}

fn coupling(from: OsKind, to: OsKind, p: &CrossTalkParams) -> Option<EdgeCoupling> {
    let secretion = p.adipose_secretion;
    let growth = p.igf1_growth;
    match (from, to) {
        // Adipose -> IGF1: secretion proportional to adipose activity.
        (OsKind::AdiposeTissue, OsKind::Igf1) => {
            Some(Box::new(move |_: f64, y_from: &[f64], _: &[f64]| {
                vec![secretion * y_from[0]]
            }))
        }
        // IGF1 -> Breast: logistic growth of proliferation under IGF-1 exposure.
        (OsKind::Igf1, OsKind::BreastTissue) => {
            Some(Box::new(move |_: f64, y_from: &[f64], y_to: &[f64]| {
                vec![growth * y_from[0] * (1.0 - y_to[0].clamp(0.0, 1.0))]
            }))
        }
        _ => None,
    }
}

fn initial_value(kind: OsKind) -> f64 {
    match kind {
        OsKind::AdiposeTissue => 1.0,
        OsKind::Igf1 | OsKind::BreastTissue => 0.0,
    }
}

/// Build a coupled system from an OSG topology.
///
/// Node order in `topology.nodes` fixes the state layout (one scalar per node).
/// Returns the system and its default initial state. Fails on unknown node
/// kinds, duplicate or dangling node ids, non-`cross_talk` edges, and edges
/// between kinds with no defined coupling.
pub fn build_from_topology(
    topology: &OsTopology,
    params: &CrossTalkParams,
) -> Result<(CrossTalkSystem, Vec<f64>)> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut kinds = Vec::with_capacity(topology.nodes.len());
    let mut nodes = Vec::with_capacity(topology.nodes.len());
    let mut init = Vec::with_capacity(topology.nodes.len());

    for row in &topology.nodes {
        if index.contains_key(&row.id) {
            return Err(SimulacrumError::InvalidInput(format!(
                "duplicate OSG node id: {}",
                row.id
            )));
        }
        let kind = OsKind::parse(&row.kind)?;
        index.insert(row.id.clone(), nodes.len());
        kinds.push(kind);
        nodes.push(OsNode {
            id: row.id.clone(),
            dim: 1,
            local: local_dynamics(kind, params),
        });
        init.push(initial_value(kind));
    }

    let mut edges = Vec::with_capacity(topology.edges.len());
    for row in &topology.edges {
        if row.edge_type != CROSS_TALK_EDGE_TYPE {
            return Err(SimulacrumError::InvalidInput(format!(
                "unsupported OSG edge type: {}",
                row.edge_type
            )));
        }
        let lookup = |id: &String| {
            index.get(id).copied().ok_or_else(|| {
                SimulacrumError::InvalidInput(format!("edge references unknown OSG node: {id}"))
            })
        };
        let from = lookup(&row.from)?;
        let to = lookup(&row.to)?;
        let c = coupling(kinds[from], kinds[to], params).ok_or_else(|| {
            SimulacrumError::InvalidInput(format!(
                "no cross-talk coupling defined from {} to {}",
                row.from, row.to
            ))
        })?;
        edges.push(OsEdge {
            from,
            to,
            coupling: c,
        });
    }

    Ok((CrossTalkSystem::new(nodes, edges), init))
}

/// The canonical adipose → IGF-1 → breast-tissue topology, as stored in the OSG.
pub fn adipose_igf1_breast_topology() -> OsTopology {
    let node = |id: &str, kind: &str| OsNodeRow {
        id: id.to_string(),
        kind: kind.to_string(),
    };
    let edge = |from: &str, to: &str| OsEdgeRow {
        from: from.to_string(),
        to: to.to_string(),
        edge_type: CROSS_TALK_EDGE_TYPE.to_string(),
    };
    OsTopology {
        nodes: vec![
            node("AdiposeTissue", "AdiposeTissue"),
            node("IGF1", "IGF1"),
            node("BreastTissue", "BreastTissue"),
        ],
        edges: vec![edge("AdiposeTissue", "IGF1"), edge("IGF1", "BreastTissue")],
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
    let params = CrossTalkParams {
        adipose_secretion,
        igf1_clearance,
        igf1_growth,
    };
    build_from_topology(&adipose_igf1_breast_topology(), &params)
        .expect("canonical adipose-IGF1-breast topology is valid")
}

/// A systemic query: simulate an OSG topology and report on one target node.
#[derive(Debug, Clone)]
pub struct SystemicQuery {
    pub topology: OsTopology,
    pub params: CrossTalkParams,
    /// Id of the OSG node whose trajectory is reported.
    pub target_node_id: String,
    pub t0: f64,
    pub dt: f64,
    pub steps: usize,
}

/// Summary of the target node's trajectory over the query window.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemicQueryResult {
    pub target_node_id: String,
    pub final_value: f64,
    pub peak_value: f64,
    pub peak_time: f64,
    pub samples: usize,
}

/// Run a systemic query. Nothing is persisted; the caller decides what to store.
pub fn run_systemic_query(query: &SystemicQuery) -> Result<SystemicQueryResult> {
    if query.dt <= 0.0 || query.steps == 0 {
        return Err(SimulacrumError::InvalidInput(
            "dt must be positive and steps must be > 0".to_string(),
        ));
    }
    let target = query
        .topology
        .nodes
        .iter()
        .position(|n| n.id == query.target_node_id)
        .ok_or_else(|| {
            SimulacrumError::InvalidInput(format!(
                "target node not in topology: {}",
                query.target_node_id
            ))
        })?;
    let (system, init) = build_from_topology(&query.topology, &query.params)?;
    let traj = system.simulate(query.t0, &init, query.dt, query.steps);
    let offset = system.node_offset(target);
    let (peak_time, peak_value) = traj.iter().map(|(t, y)| (*t, y[offset])).fold(
        (query.t0, f64::NEG_INFINITY),
        |best, cur| {
            if cur.1 > best.1 { cur } else { best }
        },
    );
    let final_value = traj.last().map(|(_, y)| y[offset]).unwrap_or(init[target]);
    Ok(SystemicQueryResult {
        target_node_id: query.target_node_id.clone(),
        final_value,
        peak_value,
        peak_time,
        samples: traj.len(),
    })
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

    #[test]
    fn test_topology_builder_matches_canonical_example() {
        let params = CrossTalkParams {
            adipose_secretion: 0.8,
            igf1_clearance: 0.2,
            igf1_growth: 0.5,
        };
        let (from_topology, init) =
            build_from_topology(&adipose_igf1_breast_topology(), &params).unwrap();
        let (canonical, canonical_init) = build_adipose_igf1_breast(0.8, 0.2, 0.5);
        assert_eq!(init, canonical_init);
        let a = from_topology.simulate(0.0, &init, 0.05, 200);
        let b = canonical.simulate(0.0, &canonical_init, 0.05, 200);
        assert_eq!(a.last().unwrap().1, b.last().unwrap().1);
    }

    #[test]
    fn test_topology_builder_rejects_invalid_topologies() {
        let params = CrossTalkParams::default();
        let canonical = adipose_igf1_breast_topology();

        let mut unknown_kind = canonical.clone();
        unknown_kind.nodes[1].kind = "Liver".to_string();
        assert!(build_from_topology(&unknown_kind, &params).is_err());

        let mut duplicate = canonical.clone();
        duplicate.nodes[2].id = "IGF1".to_string();
        assert!(build_from_topology(&duplicate, &params).is_err());

        let mut dangling = canonical.clone();
        dangling.edges[0].to = "Missing".to_string();
        assert!(build_from_topology(&dangling, &params).is_err());

        let mut wrong_type = canonical.clone();
        wrong_type.edges[0].edge_type = "affects".to_string();
        assert!(build_from_topology(&wrong_type, &params).is_err());

        // Breast -> adipose has no defined coupling.
        let mut no_coupling = canonical.clone();
        no_coupling.edges.push(OsEdgeRow {
            from: "BreastTissue".to_string(),
            to: "AdiposeTissue".to_string(),
            edge_type: CROSS_TALK_EDGE_TYPE.to_string(),
        });
        assert!(build_from_topology(&no_coupling, &params).is_err());
    }
}
