import { useState } from 'react';
import TrajectoryChart, { TrajectoryBand, TrajectoryPoint } from './TrajectoryChart';

interface ClinicalObservation {
  subject_id: string;
  loinc_code: string;
  value: number;
  unit: string | null;
  effective_time: string;
  status: string;
  source: string;
}

interface CgmReading {
  subject_id: string;
  ts: string;
  glucose_mgdl: number;
  source: string;
}

interface GlycemicVariability {
  readings_count: number;
  cv: number;
  sd: number;
  mean_glucose: number;
  mage: number;
  gmi: number;
  tir: { in_range_pct: number };
}

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:8080';

// CGM Time-in-Range bands (standard ADA/ATTD consensus ranges), colored with the
// reserved status palette (never reused for series identity elsewhere in the app).
const CGM_BANDS: TrajectoryBand[] = [
  { from: 0, to: 54, color: '#d03b3b', label: 'Very low (<54)' },
  { from: 54, to: 70, color: '#ec835a', label: 'Low (54-69)' },
  { from: 70, to: 180, color: '#0ca30c', label: 'In range (70-180)' },
  { from: 180, to: 250, color: '#fab219', label: 'High (181-250)' },
  { from: 250, to: 400, color: '#d03b3b', label: 'Very high (>250)' },
];

function PhysiologyPanel({ token }: { token: string }) {
  const [subjectId, setSubjectId] = useState('');
  const [loincCode, setLoincCode] = useState('2160-0');
  const [start, setStart] = useState('');
  const [end, setEnd] = useState('');

  const [observations, setObservations] = useState<ClinicalObservation[]>([]);
  const [cgmReadings, setCgmReadings] = useState<CgmReading[]>([]);
  const [variability, setVariability] = useState<GlycemicVariability | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const authHeaders = () => ({
    Authorization: `Bearer ${token}`,
    'Content-Type': 'application/json',
  });

  const loadTrajectory = async () => {
    if (!subjectId || !loincCode) return;
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(
        `${API_BASE}/api/v1/clinical-observations/${subjectId}`,
        { headers: authHeaders() },
      );
      if (!response.ok) throw new Error(await response.text());
      const data: ClinicalObservation[] = await response.json();
      setObservations(data.filter(o => o.loinc_code === loincCode));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load organ function trajectory');
    } finally {
      setLoading(false);
    }
  };

  const loadCgm = async () => {
    if (!subjectId) return;
    setLoading(true);
    setError(null);
    try {
      const readingsResp = await fetch(`${API_BASE}/api/v1/cgm/${subjectId}`, {
        headers: authHeaders(),
      });
      if (!readingsResp.ok) throw new Error(await readingsResp.text());
      setCgmReadings(await readingsResp.json());

      if (start && end) {
        const params = new URLSearchParams({ start, end });
        const varResp = await fetch(
          `${API_BASE}/api/v1/cgm/${subjectId}/variability?${params.toString()}`,
          { headers: authHeaders() },
        );
        if (varResp.ok) {
          setVariability(await varResp.json());
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load CGM data');
    } finally {
      setLoading(false);
    }
  };

  const trajectoryPoints: TrajectoryPoint[] = observations.map(o => ({
    timestamp: o.effective_time,
    value: o.value,
  }));

  const cgmPoints: TrajectoryPoint[] = cgmReadings.map(r => ({
    timestamp: r.ts,
    value: r.glucose_mgdl,
  }));

  return (
    <section className="physiology-panel">
      <h2>Phase 2: Physiological &amp; Temporal</h2>
      <p className="hint">
        Longitudinal organ function trajectories and CGM time-in-range, keyed by subject ID
        (clinical/EHR data, distinct from the Phase 1 sample_id above).
      </p>

      <div className="physiology-inputs">
        <input
          placeholder="Subject ID"
          value={subjectId}
          onChange={e => setSubjectId(e.target.value)}
        />
        <input
          placeholder="LOINC code (e.g. 2160-0 = creatinine)"
          value={loincCode}
          onChange={e => setLoincCode(e.target.value)}
        />
        <button onClick={loadTrajectory} disabled={loading || !subjectId || !loincCode}>
          Load Trajectory
        </button>
        <input
          type="datetime-local"
          value={start}
          onChange={e => setStart(e.target.value)}
          title="Variability period start"
        />
        <input
          type="datetime-local"
          value={end}
          onChange={e => setEnd(e.target.value)}
          title="Variability period end"
        />
        <button onClick={loadCgm} disabled={loading || !subjectId}>
          Load CGM
        </button>
      </div>

      {error && <div className="error">{error}</div>}
      {loading && <div className="loading">Loading...</div>}

      <div className="physiology-charts">
        {trajectoryPoints.length > 0 && (
          <TrajectoryChart
            title={`Organ function trajectory — LOINC ${loincCode}`}
            unit={observations[0]?.unit || ''}
            points={trajectoryPoints}
          />
        )}

        {cgmPoints.length > 0 && (
          <>
            {variability && (
              <div className="variability-metrics">
                <div className="variability-metric">
                  <span className="value">{variability.tir.in_range_pct.toFixed(0)}%</span>
                  <span className="label">Time in range</span>
                </div>
                <div className="variability-metric">
                  <span className="value">{variability.cv.toFixed(1)}%</span>
                  <span className="label">CV</span>
                </div>
                <div className="variability-metric">
                  <span className="value">{variability.mean_glucose.toFixed(0)}</span>
                  <span className="label">Mean mg/dL</span>
                </div>
                <div className="variability-metric">
                  <span className="value">{variability.mage.toFixed(0)}</span>
                  <span className="label">MAGE</span>
                </div>
                <div className="variability-metric">
                  <span className="value">{variability.gmi.toFixed(1)}%</span>
                  <span className="label">GMI (est. HbA1c)</span>
                </div>
              </div>
            )}
            <TrajectoryChart
              title="Continuous glucose monitoring"
              unit="mg/dL"
              points={cgmPoints}
              bands={CGM_BANDS}
              lineColor="#2a78d6"
            />
          </>
        )}
      </div>
    </section>
  );
}

export default PhysiologyPanel;
