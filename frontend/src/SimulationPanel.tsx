import { useState } from 'react';
import TrajectoryChart, { TrajectoryPoint } from './TrajectoryChart';

interface SimulationOutput {
  run_id: string;
  ts: string;
  series_name: string;
  value: number;
}

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:8080';

const SERIES_COLORS: Record<string, string> = {
  adipose: '#8a5a2b',
  igf1: '#2a78d6',
  breast: '#0ca30c',
};

function SimulationPanel({ token }: { token: string }) {
  const [subjectId, setSubjectId] = useState('');
  const [t0, setT0] = useState('0');
  const [dt, setDt] = useState('0.05');
  const [steps, setSteps] = useState('600');
  const [adiposeSecretion, setAdiposeSecretion] = useState('0.8');
  const [igf1Clearance, setIgf1Clearance] = useState('0.2');
  const [igf1Growth, setIgf1Growth] = useState('0.5');

  const [runId, setRunId] = useState<string | null>(null);
  const [outputs, setOutputs] = useState<SimulationOutput[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const authHeaders = () => ({
    Authorization: `Bearer ${token}`,
    'Content-Type': 'application/json',
  });

  const runSimulation = async () => {
    if (!subjectId) {
      setError('Provide a subject_id before running a simulation.');
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(`${API_BASE}/api/v1/simulate`, {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          subject_id: subjectId,
          t0: Number(t0),
          dt: Number(dt),
          steps: Number(steps),
          adipose_secretion: Number(adiposeSecretion),
          igf1_clearance: Number(igf1Clearance),
          igf1_growth: Number(igf1Growth),
        }),
      });
      if (!res.ok) throw new Error(await res.text());
      const data = await res.json();
      setRunId(data.run_id);

      const outRes = await fetch(`${API_BASE}/api/v1/simulations/${data.run_id}`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (!outRes.ok) throw new Error(await outRes.text());
      setOutputs(await outRes.json());
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Simulation failed');
    } finally {
      setLoading(false);
    }
  };

  const seriesNames = [...new Set(outputs.map((o) => o.series_name))].sort();

  return (
    <section className="simulation-panel">
      <h2>Simulation (Phase 3 digital twin)</h2>
      <p className="hint">
        Run the OSG cross-talk model (adipose → IGF-1 → breast-tissue) and inspect the
        resulting trajectories. Outputs are stored under the <code>simulation_output</code>{' '}
        data class and exported only through the differential-privacy-guarded aggregate path.
      </p>

      <div className="simulation-inputs">
        <label>
          Subject ID
          <input value={subjectId} onChange={(e) => setSubjectId(e.target.value)} placeholder="subject-1" />
        </label>
        <label>
          t0
          <input value={t0} onChange={(e) => setT0(e.target.value)} />
        </label>
        <label>
          dt
          <input value={dt} onChange={(e) => setDt(e.target.value)} />
        </label>
        <label>
          steps
          <input value={steps} onChange={(e) => setSteps(e.target.value)} />
        </label>
        <label>
          adipose_secretion
          <input value={adiposeSecretion} onChange={(e) => setAdiposeSecretion(e.target.value)} />
        </label>
        <label>
          igf1_clearance
          <input value={igf1Clearance} onChange={(e) => setIgf1Clearance(e.target.value)} />
        </label>
        <label>
          igf1_growth
          <input value={igf1Growth} onChange={(e) => setIgf1Growth(e.target.value)} />
        </label>
        <button onClick={runSimulation} disabled={loading}>
          {loading ? 'Simulating...' : 'Run simulation'}
        </button>
      </div>

      {error && <div className="error">{error}</div>}
      {runId && <p className="hint">Run <code>{runId}</code> — {outputs.length} output points.</p>}

      {seriesNames.map((name) => {
        const points: TrajectoryPoint[] = outputs
          .filter((o) => o.series_name === name)
          .map((o) => ({ timestamp: o.ts, value: o.value }));
        return (
          <TrajectoryChart
            key={name}
            title={name}
            unit=""
            points={points}
            lineColor={SERIES_COLORS[name] ?? '#2a78d6'}
          />
        );
      })}
    </section>
  );
}

export default SimulationPanel;
