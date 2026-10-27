import { useState } from 'react';
import {
  CANONICAL_QUERY,
  OSG_KINDS,
  buildSystemicRequest,
  validateQuery,
  type OsgKind,
  type QueryForm,
  type SystemicQueryResult,
} from './systemicQuery';

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:8080';

type NumericField = 't0' | 'dt' | 'steps' | 'adiposeSecretion' | 'igf1Clearance' | 'igf1Growth';

function SystemicQueryPanel({ token }: { token: string }) {
  const [form, setForm] = useState<QueryForm>(CANONICAL_QUERY);
  const [result, setResult] = useState<SystemicQueryResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const problem = validateQuery(form);

  const setField = <K extends keyof QueryForm>(key: K, value: QueryForm[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  // Renaming a node also rewrites the edges and target that point at it, so the
  // topology stays connected while the user types a new id.
  const renameNode = (index: number, id: string) =>
    setForm((f) => {
      const old = f.nodes[index].id;
      const rename = (v: string) => (v === old ? id : v);
      return {
        ...f,
        nodes: f.nodes.map((n, i) => (i === index ? { ...n, id } : n)),
        edges: f.edges.map((e) => ({ from: rename(e.from), to: rename(e.to) })),
        targetNodeId: rename(f.targetNodeId),
      };
    });

  const setNodeKind = (index: number, kind: OsgKind) =>
    setForm((f) => ({
      ...f,
      nodes: f.nodes.map((n, i) => (i === index ? { ...n, kind } : n)),
    }));

  const addNode = () =>
    setForm((f) => {
      let n = f.nodes.length + 1;
      while (f.nodes.some((node) => node.id === `node-${n}`)) n++;
      return { ...f, nodes: [...f.nodes, { id: `node-${n}`, kind: 'BreastTissue' }] };
    });

  // Dropping a node also drops the edges that reference it. A deleted target is
  // cleared so the user must choose a new one.
  const removeNode = (index: number) =>
    setForm((f) => {
      const gone = f.nodes[index].id;
      return {
        ...f,
        nodes: f.nodes.filter((_, i) => i !== index),
        edges: f.edges.filter((e) => e.from !== gone && e.to !== gone),
        targetNodeId: f.targetNodeId === gone ? '' : f.targetNodeId,
      };
    });

  const addEdge = () =>
    setForm((f) => {
      const first = f.nodes[0]?.id ?? '';
      const last = f.nodes[f.nodes.length - 1]?.id ?? '';
      return { ...f, edges: [...f.edges, { from: first, to: last }] };
    });

  const updateEdge = (index: number, patch: Partial<QueryForm['edges'][number]>) =>
    setForm((f) => ({
      ...f,
      edges: f.edges.map((e, i) => (i === index ? { ...e, ...patch } : e)),
    }));

  const removeEdge = (index: number) =>
    setForm((f) => ({ ...f, edges: f.edges.filter((_, i) => i !== index) }));

  const numberInput = (key: NumericField, label: string, step = 'any') => (
    <label>
      {label}
      <input
        type="number"
        step={step}
        value={form[key]}
        onChange={(e) => setField(key, Number(e.target.value))}
      />
    </label>
  );

  const runQuery = async () => {
    let body;
    try {
      body = buildSystemicRequest(form);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid query');
      return;
    }
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const res = await fetch(`${API_BASE}/api/v1/systemic/query`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(body),
      });
      if (!res.ok) throw new Error(await res.text());
      setResult(await res.json());
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Systemic query failed');
    } finally {
      setLoading(false);
    }
  };

  const nodeIds = form.nodes.map((n) => n.id);

  return (
    <section className="systemic-panel">
      <h2>Systemic query (Phase 4 OSG)</h2>
      <p className="hint">
        Build an Ontological Soma Graph topology, choose a target node, and run the cross-talk
        solver. The result is computed for this request only and nothing is stored.
      </p>

      <div className="systemic-actions">
        <button
          type="button"
          onClick={() => {
            setForm(CANONICAL_QUERY);
            setResult(null);
            setError(null);
          }}
        >
          Load canonical example
        </button>
      </div>

      <h3>Nodes</h3>
      <table className="systemic-table">
        <thead>
          <tr>
            <th>Id</th>
            <th>Kind</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {form.nodes.map((node, i) => (
            <tr key={i}>
              <td>
                <input value={node.id} onChange={(e) => renameNode(i, e.target.value)} />
              </td>
              <td>
                <select value={node.kind} onChange={(e) => setNodeKind(i, e.target.value as OsgKind)}>
                  {OSG_KINDS.map((kind) => (
                    <option key={kind} value={kind}>
                      {kind}
                    </option>
                  ))}
                </select>
              </td>
              <td>
                <button type="button" onClick={() => removeNode(i)} aria-label={`Remove node ${node.id}`}>
                  Remove
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <button type="button" onClick={addNode}>
        Add node
      </button>

      <h3>Cross-talk edges</h3>
      <table className="systemic-table">
        <thead>
          <tr>
            <th>From</th>
            <th>To</th>
            <th>Edge type</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {form.edges.map((edge, i) => (
            <tr key={i}>
              <td>
                <select value={edge.from} onChange={(e) => updateEdge(i, { from: e.target.value })}>
                  {nodeIds.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
              </td>
              <td>
                <select value={edge.to} onChange={(e) => updateEdge(i, { to: e.target.value })}>
                  {nodeIds.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
              </td>
              <td>cross_talk</td>
              <td>
                <button type="button" onClick={() => removeEdge(i)} aria-label={`Remove edge ${i + 1}`}>
                  Remove
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <button type="button" onClick={addEdge} disabled={form.nodes.length === 0}>
        Add edge
      </button>

      <h3>Target and window</h3>
      <div className="systemic-inputs">
        <label>
          Target node
          <select
            value={form.targetNodeId}
            onChange={(e) => setField('targetNodeId', e.target.value)}
          >
            {form.targetNodeId === '' && <option value="">Choose a node</option>}
            {nodeIds.map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
        </label>
        {numberInput('t0', 't0')}
        {numberInput('dt', 'dt')}
        {numberInput('steps', 'steps', '1')}
      </div>

      <h3>Parameters</h3>
      <div className="systemic-inputs">
        {numberInput('adiposeSecretion', 'adipose_secretion')}
        {numberInput('igf1Clearance', 'igf1_clearance')}
        {numberInput('igf1Growth', 'igf1_growth')}
      </div>

      <div className="systemic-actions">
        <button type="button" onClick={runQuery} disabled={loading || problem !== null}>
          {loading ? 'Running...' : 'Run systemic query'}
        </button>
      </div>
      {problem && <p className="hint">{problem}</p>}
      {error && <div className="error">{error}</div>}

      {result && (
        <dl className="systemic-result">
          <dt>Target</dt>
          <dd>{result.target_node_id}</dd>
          <dt>Final value</dt>
          <dd>{result.final_value.toFixed(4)}</dd>
          <dt>Peak value</dt>
          <dd>{result.peak_value.toFixed(4)}</dd>
          <dt>Peak time</dt>
          <dd>{result.peak_time.toFixed(2)}</dd>
          <dt>Samples</dt>
          <dd>{result.samples}</dd>
        </dl>
      )}
    </section>
  );
}

export default SystemicQueryPanel;
