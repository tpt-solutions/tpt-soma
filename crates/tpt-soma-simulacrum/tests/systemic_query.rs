//! In-process systemic query over an OSG-shaped topology (ADR 007 §2.9).
//!
//! The DB-backed variant reads these rows from Plexus; this test feeds the same
//! row shapes directly so the query path runs without a live Keystone instance.

use tpt_soma_simulacrum::crosstalk::{
    CROSS_TALK_EDGE_TYPE, CrossTalkParams, OsEdgeRow, OsNodeRow, OsTopology, build_from_topology,
};

fn node(id: &str, kind: &str) -> OsNodeRow {
    OsNodeRow {
        id: id.to_string(),
        kind: kind.to_string(),
    }
}

fn cross_talk(from: &str, to: &str) -> OsEdgeRow {
    OsEdgeRow {
        from: from.to_string(),
        to: to.to_string(),
        edge_type: CROSS_TALK_EDGE_TYPE.to_string(),
    }
}

fn adipose_igf1_breast_rows() -> OsTopology {
    OsTopology {
        nodes: vec![
            node("adipose-visceral-1", "AdiposeTissue"),
            node("igf1-plasma", "IGF1"),
            node("breast-left", "BreastTissue"),
        ],
        edges: vec![
            cross_talk("adipose-visceral-1", "igf1-plasma"),
            cross_talk("igf1-plasma", "breast-left"),
        ],
    }
}

#[test]
fn systemic_query_adipose_igf1_breast_end_to_end() {
    let params = CrossTalkParams::default();
    let coupled_topology = adipose_igf1_breast_rows();
    let (coupled, init) = build_from_topology(&coupled_topology, &params).unwrap();

    let coupled_traj = coupled.simulate(0.0, &init, 0.05, 600);
    let (_, coupled_last) = coupled_traj.last().unwrap();
    let igf1 = coupled_last[coupled.node_offset(1)];
    let breast = coupled_last[coupled.node_offset(2)];

    // Adipose secretion must produce IGF-1, and IGF-1 must drive breast proliferation.
    assert!(igf1 > 0.0, "IGF-1 should be produced, got {igf1}");
    assert!(
        breast > 0.0,
        "breast proliferation should be driven, got {breast}"
    );

    // Control: identical nodes with the cross_talk edges removed.
    let control_topology = OsTopology {
        nodes: coupled_topology.nodes.clone(),
        edges: vec![],
    };
    let (control, control_init) = build_from_topology(&control_topology, &params).unwrap();
    let control_traj = control.simulate(0.0, &control_init, 0.05, 600);
    let (_, control_last) = control_traj.last().unwrap();
    let control_breast = control_last[control.node_offset(2)];

    assert!(
        control_breast.abs() < 1e-12,
        "uncoupled breast should stay at 0"
    );
    assert!(breast > control_breast);
}

#[test]
fn systemic_query_reports_target_node_trajectory() {
    use tpt_soma_simulacrum::crosstalk::{SystemicQuery, run_systemic_query};

    let query = SystemicQuery {
        topology: adipose_igf1_breast_rows(),
        params: CrossTalkParams::default(),
        target_node_id: "breast-left".to_string(),
        t0: 0.0,
        dt: 0.05,
        steps: 600,
    };
    let result = run_systemic_query(&query).unwrap();
    assert_eq!(result.target_node_id, "breast-left");
    assert_eq!(result.samples, 601);
    assert!(result.final_value > 0.0);
    assert!(result.peak_value >= result.final_value);
    // Window is 600 steps of 0.05; allow for accumulated floating-point drift.
    assert!(result.peak_time >= 0.0 && result.peak_time <= 30.0 + 1e-9);
}

#[test]
fn systemic_query_rejects_unknown_target_and_bad_window() {
    use tpt_soma_simulacrum::crosstalk::{SystemicQuery, run_systemic_query};

    let base = SystemicQuery {
        topology: adipose_igf1_breast_rows(),
        params: CrossTalkParams::default(),
        target_node_id: "breast-left".to_string(),
        t0: 0.0,
        dt: 0.05,
        steps: 10,
    };
    let unknown = SystemicQuery {
        target_node_id: "liver".to_string(),
        ..base.clone()
    };
    assert!(run_systemic_query(&unknown).is_err());

    let zero_dt = SystemicQuery {
        dt: 0.0,
        ..base.clone()
    };
    assert!(run_systemic_query(&zero_dt).is_err());

    let zero_steps = SystemicQuery { steps: 0, ..base };
    assert!(run_systemic_query(&zero_steps).is_err());
}

#[test]
fn systemic_query_is_deterministic_for_same_topology() {
    let params = CrossTalkParams::default();
    let topology = adipose_igf1_breast_rows();
    let (a, init_a) = build_from_topology(&topology, &params).unwrap();
    let (b, init_b) = build_from_topology(&topology, &params).unwrap();
    assert_eq!(
        a.simulate(0.0, &init_a, 0.05, 200).last().unwrap().1,
        b.simulate(0.0, &init_b, 0.05, 200).last().unwrap().1
    );
}
