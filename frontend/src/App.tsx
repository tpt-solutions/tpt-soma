import React, { useState, useEffect } from 'react';
import './App.css';

interface Sample {
  sample_id: string;
  patient_id: string | null;
  source: string;
  dataset_provenance: string | null;
}

interface Variant {
  variant_id: string;
  chromosome: string;
  position: number;
  reference: string;
  alternate: string;
  rsid: string | null;
  clinvar_id: string | null;
  genotype: string | null;
}

interface UmapPoint {
  cell_id: string;
  umap1: number;
  umap2: number;
  cluster: string;
}

interface Cohort {
  cohort_id: string;
  name: string;
  description: string | null;
}

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:8080';

function App() {
  const [token, setToken] = useState<string>('');
  const [cohorts, setCohorts] = useState<Cohort[]>([]);
  const [selectedCohort, setSelectedCohort] = useState<string>('');
  const [samples, setSamples] = useState<Sample[]>([]);
  const [selectedSample, setSelectedSample] = useState<string>('');
  const [variants, setVariants] = useState<Variant[]>([]);
  const [umapData, setUmapData] = useState<UmapPoint[]>([]);
  const [clusters, setClusters] = useState<string[]>([]);
  const [selectedCluster, setSelectedCluster] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'variants' | 'umap' | 'join'>('variants');

  const authHeaders = () => ({
    'Authorization': `Bearer ${token}`,
    'Content-Type': 'application/json',
  });

  const fetchWithAuth = async (url: string, options: RequestInit = {}) => {
    const response = await fetch(`${API_BASE}${url}`, {
      ...options,
      headers: {
        ...authHeaders(),
        ...options.headers,
      },
    });
    if (!response.ok) {
      const text = await response.text();
      throw new Error(`${response.status}: ${text}`);
    }
    return response.json();
  };

  const loadCohorts = async () => {
    try {
      // In a real app, this would be an API call
      // For now, we'll use a mock
      setCohorts([
        { cohort_id: 'cohort-1', name: '1000 Genomes Subset', description: 'Public reference data' },
        { cohort_id: 'cohort-2', name: 'PBMC 3k', description: '10x Genomics public scRNA-seq' },
      ]);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load cohorts');
    }
  };

  const loadSamples = async () => {
    if (!selectedCohort) return;
    setLoading(true);
    setError(null);
    try {
      const data = await fetchWithAuth(`/api/v1/cohorts/${selectedCohort}/samples`);
      setSamples(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load samples');
    } finally {
      setLoading(false);
    }
  };

  const loadVariants = async () => {
    if (!selectedSample) return;
    setLoading(true);
    setError(null);
    try {
      const data = await fetchWithAuth(`/api/v1/variants/${selectedSample}`);
      setVariants(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load variants');
    } finally {
      setLoading(false);
    }
  };

  const loadUmap = async () => {
    if (!selectedSample) return;
    setLoading(true);
    setError(null);
    try {
      const data = await fetchWithAuth(`/api/v1/umap/${selectedSample}`);
      setUmapData(data);
      const uniqueClusters = [...new Set(data.map((d: UmapPoint) => d.cluster))].sort() as string[];
      setClusters(uniqueClusters);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load UMAP data');
    } finally {
      setLoading(false);
    }
  };

  const handleSampleChange = (sampleId: string) => {
    setSelectedSample(sampleId);
    setVariants([]);
    setUmapData([]);
    setClusters([]);
    setSelectedCluster('');
    loadVariants();
    loadUmap();
  };

  const filteredUmap = selectedCluster 
    ? umapData.filter(d => d.cluster === selectedCluster)
    : umapData;

  useEffect(() => {
    loadCohorts();
  }, []);

  useEffect(() => {
    loadSamples();
  }, [selectedCohort]);

  return (
    <div className="app">
      <header>
        <h1>tpt-soma</h1>
        <p>Multi-scale computational physiology platform</p>
      </header>

      <div className="token-input">
        <label>
          Capability Token:
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="Paste your capability token here"
          />
        </label>
      </div>

      {error && <div className="error">{error}</div>}

      <div className="main-content">
        <aside className="sidebar">
          <section>
            <h2>Cohort</h2>
            <select
              value={selectedCohort}
              onChange={(e) => setSelectedCohort(e.target.value)}
              disabled={loading}
            >
              <option value="">Select cohort...</option>
              {cohorts.map(c => (
                <option key={c.cohort_id} value={c.cohort_id}>
                  {c.name}
                </option>
              ))}
            </select>
          </section>

          <section>
            <h2>Samples</h2>
            {loading ? (
              <div className="loading">Loading...</div>
            ) : (
              <ul className="sample-list">
                {samples.map(s => (
                  <li
                    key={s.sample_id}
                    className={selectedSample === s.sample_id ? 'selected' : ''}
                    onClick={() => handleSampleChange(s.sample_id)}
                  >
                    {s.sample_id.slice(0, 8)}... ({s.source})
                  </li>
                ))}
              </ul>
            )}
          </section>
        </aside>

        <main className="content">
          {selectedSample ? (
            <>
              <div className="tabs">
                <button
                  className={activeTab === 'variants' ? 'active' : ''}
                  onClick={() => setActiveTab('variants')}
                >
                  Variants
                </button>
                <button
                  className={activeTab === 'umap' ? 'active' : ''}
                  onClick={() => setActiveTab('umap')}
                >
                  UMAP / Clusters
                </button>
                <button
                  className={activeTab === 'join' ? 'active' : ''}
                  onClick={() => setActiveTab('join')}
                >
                  Multi-omics Join
                </button>
              </div>

              {activeTab === 'variants' && (
                <VariantTable variants={variants} loading={loading} />
              )}

              {activeTab === 'umap' && (
                <UmapViewer
                  data={filteredUmap}
                  clusters={clusters}
                  selectedCluster={selectedCluster}
                  onClusterChange={setSelectedCluster}
                  loading={loading}
                />
              )}

              {activeTab === 'join' && (
                <JoinViewer sampleId={selectedSample} token={token} />
              )}
            </>
          ) : (
            <div className="placeholder">
              <p>Select a cohort and sample to view data.</p>
              <p className="hint">Phase 1: VCF variant ingestion + scRNA-seq (10x/AnnData) + minimal variant↔expression join</p>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

function VariantTable({ variants, loading }: { variants: Variant[]; loading: boolean }) {
  if (loading) return <div className="loading">Loading variants...</div>;
  if (variants.length === 0) return <p>No variants found for this sample.</p>;

  return (
    <div className="table-container">
      <table>
        <thead>
          <tr>
            <th>Variant ID</th>
            <th>Chr</th>
            <th>Pos</th>
            <th>Ref</th>
            <th>Alt</th>
            <th>rsID</th>
            <th>ClinVar</th>
            <th>Genotype</th>
          </tr>
        </thead>
        <tbody>
          {variants.map(v => (
            <tr key={v.variant_id}>
              <td>{v.variant_id}</td>
              <td>{v.chromosome}</td>
              <td>{v.position}</td>
              <td>{v.reference}</td>
              <td>{v.alternate}</td>
              <td>{v.rsid || '-'}</td>
              <td>{v.clinvar_id || '-'}</td>
              <td>{v.genotype || '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function UmapViewer({
  data,
  clusters,
  selectedCluster,
  onClusterChange,
  loading,
}: {
  data: UmapPoint[];
  clusters: string[];
  selectedCluster: string;
  onClusterChange: (cluster: string) => void;
  loading: boolean;
}) {
  if (loading) return <div className="loading">Loading UMAP...</div>;
  if (data.length === 0) return <p>No UMAP data for this sample.</p>;

  // Simple canvas-based scatter plot
  const canvasRef = React.useRef<HTMLCanvasElement>(null);
  
  React.useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const width = canvas.width = canvas.clientWidth;
    const height = canvas.height = canvas.clientHeight;
    const padding = 40;

    // Find bounds
    const xValues = data.map(d => d.umap1);
    const yValues = data.map(d => d.umap2);
    const xMin = Math.min(...xValues);
    const xMax = Math.max(...xValues);
    const yMin = Math.min(...yValues);
    const yMax = Math.max(...yValues);

    const xScale = (width - 2 * padding) / (xMax - xMin || 1);
    const yScale = (height - 2 * padding) / (yMax - yMin || 1);

    // Color map for clusters
    const colors: Record<string, string> = {};
    const colorPalette = [
      '#e41a1c', '#377eb8', '#4daf4a', '#984ea3', '#ff7f00',
      '#ffff33', '#a65628', '#f781bf', '#999999', '#66c2a5'
    ];
    clusters.forEach((c, i) => {
      colors[c] = colorPalette[i % colorPalette.length];
    });

    ctx.clearRect(0, 0, width, height);

    // Draw axes
    ctx.strokeStyle = '#ddd';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(padding, padding);
    ctx.lineTo(padding, height - padding);
    ctx.lineTo(width - padding, height - padding);
    ctx.stroke();

    // Draw points
    data.forEach(d => {
      const x = padding + (d.umap1 - xMin) * xScale;
      const y = height - padding - (d.umap2 - yMin) * yScale;
      ctx.fillStyle = colors[d.cluster] || '#333';
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, 2 * Math.PI);
      ctx.fill();
    });
  }, [data, clusters]);

  return (
    <div className="umap-viewer">
      <div className="controls">
        <label>
          Filter by cluster:
          <select value={selectedCluster} onChange={(e) => onClusterChange(e.target.value)}>
            <option value="">All clusters</option>
            {clusters.map(c => (
              <option key={c} value={c}>Cluster {c}</option>
            ))}
          </select>
        </label>
        <span>{data.length} cells</span>
      </div>
      <canvas ref={canvasRef} className="umap-canvas" />
      <div className="legend">
        {clusters.map(c => (
          <span key={c} className="legend-item" style={{ borderColor: `hsl(${parseInt(c) * 36}, 70%, 50%)` }}>
            Cluster {c}
          </span>
        ))}
      </div>
    </div>
  );
}

function JoinViewer({ sampleId, token }: { sampleId: string; token: string }) {
  const [variantId, setVariantId] = useState('');
  const [geneId, setGeneId] = useState('');
  const [joinData, setJoinData] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleJoin = async () => {
    if (!variantId || !geneId) return;
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(`${API_BASE}/api/v1/join/variant-expression`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ sample_id: sampleId, variant_id: variantId, gene_id: geneId }),
      });
      if (!response.ok) throw new Error(await response.text());
      const data = await response.json();
      setJoinData(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Join failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="join-viewer">
      <h3>Variant ↔ Expression Join</h3>
      <p className="hint">Enter a variant ID and gene ID to see the multi-omics join for this sample.</p>
      
      <div className="join-inputs">
        <input
          placeholder="Variant ID (e.g., 1:100:A:T)"
          value={variantId}
          onChange={(e) => setVariantId(e.target.value)}
        />
        <input
          placeholder="Gene ID (e.g., BRCA1)"
          value={geneId}
          onChange={(e) => setGeneId(e.target.value)}
        />
        <button onClick={handleJoin} disabled={loading || !variantId || !geneId}>
          {loading ? 'Joining...' : 'Join'}
        </button>
      </div>

      {error && <div className="error">{error}</div>}

      {joinData.length > 0 && (
        <div className="table-container">
          <table>
            <thead>
              <tr>
                <th>Variant ID</th>
                <th>Chr</th>
                <th>Pos</th>
                <th>Ref</th>
                <th>Alt</th>
                <th>rsID</th>
                <th>Cell ID</th>
                <th>Gene</th>
                <th>Count</th>
              </tr>
            </thead>
            <tbody>
              {joinData.map((row: any, i: number) => (
                <tr key={i}>
                  <td>{row.variant_id}</td>
                  <td>{row.chromosome}</td>
                  <td>{row.position}</td>
                  <td>{row.reference}</td>
                  <td>{row.alternate}</td>
                  <td>{row.rsid || '-'}</td>
                  <td>{row.cell_id}</td>
                  <td>{row.gene_id}</td>
                  <td>{row.count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

export default App;