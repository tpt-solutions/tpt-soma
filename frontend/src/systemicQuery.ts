/** Node kinds the OSG cross-talk solver accepts (see `OsKind::parse` in tpt-soma-simulacrum). */
export const OSG_KINDS = ['AdiposeTissue', 'IGF1', 'BreastTissue'] as const;
export type OsgKind = (typeof OSG_KINDS)[number];

/** The only edge type the solver couples over. */
export const CROSS_TALK_EDGE_TYPE = 'cross_talk';

export interface QueryNode {
  id: string;
  kind: OsgKind;
}

export interface QueryEdge {
  from: string;
  to: string;
}

export interface QueryForm {
  nodes: QueryNode[];
  edges: QueryEdge[];
  targetNodeId: string;
  t0: number;
  dt: number;
  steps: number;
  adiposeSecretion: number;
  igf1Clearance: number;
  igf1Growth: number;
}

/** Body of `POST /api/v1/systemic/query`. */
export interface SystemicQueryRequest {
  nodes: QueryNode[];
  edges: { from: string; to: string; edge_type: typeof CROSS_TALK_EDGE_TYPE }[];
  target_node_id: string;
  t0: number;
  dt: number;
  steps: number;
  adipose_secretion: number;
  igf1_clearance: number;
  igf1_growth: number;
}

/** Response of `POST /api/v1/systemic/query`. Nothing is persisted server side. */
export interface SystemicQueryResult {
  target_node_id: string;
  final_value: number;
  peak_value: number;
  peak_time: number;
  samples: number;
}

/** The spec's adipose → IGF-1 → breast-tissue example, with the defaults the simulation panel uses. */
export const CANONICAL_QUERY: QueryForm = {
  nodes: [
    { id: 'AdiposeTissue', kind: 'AdiposeTissue' },
    { id: 'IGF1', kind: 'IGF1' },
    { id: 'BreastTissue', kind: 'BreastTissue' },
  ],
  edges: [
    { from: 'AdiposeTissue', to: 'IGF1' },
    { from: 'IGF1', to: 'BreastTissue' },
  ],
  targetNodeId: 'BreastTissue',
  t0: 0,
  dt: 0.05,
  steps: 600,
  adiposeSecretion: 0.8,
  igf1Clearance: 0.2,
  igf1Growth: 0.5,
};

/**
 * Checks the form before anything is sent. The server still validates the
 * topology (dangling edges, unknown kinds, missing couplings); this catches the
 * mistakes a browser can see without a round trip.
 */
export function validateQuery(form: QueryForm): string | null {
  const ids = form.nodes.map((n) => n.id);
  if (form.nodes.length === 0) return 'Add at least one node.';
  if (ids.some((id) => id.trim() === '')) return 'Every node needs an id.';
  if (new Set(ids).size !== ids.length) return 'Node ids must be unique.';
  if (form.edges.some((e) => !ids.includes(e.from) || !ids.includes(e.to))) {
    return 'Every edge must connect two existing nodes.';
  }
  if (!ids.includes(form.targetNodeId)) return 'Pick a target node from the node list.';
  if (!Number.isFinite(form.dt) || form.dt <= 0) return 'dt must be a positive number.';
  if (!Number.isInteger(form.steps) || form.steps <= 0) return 'steps must be a positive whole number.';
  const numbers = [form.t0, form.adiposeSecretion, form.igf1Clearance, form.igf1Growth];
  if (numbers.some((v) => !Number.isFinite(v))) return 'Every parameter must be a number.';
  return null;
}

/** Turns a valid form into the request body. Throws when the form is invalid. */
export function buildSystemicRequest(form: QueryForm): SystemicQueryRequest {
  const problem = validateQuery(form);
  if (problem) throw new Error(problem);
  return {
    nodes: form.nodes.map((n) => ({ id: n.id, kind: n.kind })),
    edges: form.edges.map((e) => ({ from: e.from, to: e.to, edge_type: CROSS_TALK_EDGE_TYPE })),
    target_node_id: form.targetNodeId,
    t0: form.t0,
    dt: form.dt,
    steps: form.steps,
    adipose_secretion: form.adiposeSecretion,
    igf1_clearance: form.igf1Clearance,
    igf1_growth: form.igf1Growth,
  };
}
