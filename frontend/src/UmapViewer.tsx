import React from 'react';
import DeckGL from 'deck.gl';
import { ScatterplotLayer } from 'deck.gl/layers';

interface UmapPoint {
  cell_id: string;
  umap1: number;
  umap2: number;
  cluster: string;
}

interface UmapViewerProps {
  data: UmapPoint[];
  clusters: string[];
  selectedCluster: string;
  onClusterChange: (cluster: string) => void;
  loading: boolean;
}

const colorPalette: number[][] = [
  [228, 26, 28],
  [55, 126, 184],
  [77, 175, 74],
  [152, 78, 163],
  [255, 127, 0],
  [255, 255, 51],
  [166, 86, 40],
  [247, 129, 191],
  [153, 153, 153],
  [102, 194, 165],
];

function UmapViewer({
  data,
  clusters,
  selectedCluster,
  onClusterChange,
  loading,
}: UmapViewerProps) {
  if (loading) return <div className="loading">Loading UMAP...</div>;
  if (data.length === 0) return <p>No UMAP data for this sample.</p>;

  const filteredData = selectedCluster
    ? data.filter(d => d.cluster === selectedCluster)
    : data;

  const colorMap: Record<string, number[]> = {};
  clusters.forEach((c, i) => {
    colorMap[c] = colorPalette[i % colorPalette.length];
  });

  const xValues = data.map(d => d.umap1);
  const yValues = data.map(d => d.umap2);
  const xMin = Math.min(...xValues);
  const xMax = Math.max(...xValues);
  const yMin = Math.min(...yValues);
  const yMax = Math.max(...yValues);
  const centerX = (xMax + xMin) / 2;
  const centerY = (yMax + yMin) / 2;
  const zoom = Math.log2(800 / Math.max(xMax - xMin, yMax - yMin, 1));

  const layers = [
    new ScatterplotLayer({
      id: 'umap-scatter',
      data: filteredData,
      getPosition: (d: UmapPoint) => [d.umap1, d.umap2, 0] as [number, number, number],
      getFillColor: (d: UmapPoint) => colorMap[d.cluster] || [0, 0, 0, 255],
      getRadius: 100,
      radiusMinPixels: 3,
      radiusMaxPixels: 10,
      pickable: true,
      updateTriggers: {
        getFillColor: [selectedCluster],
      },
    }),
  ];

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
        <span>{filteredData.length} cells</span>
      </div>
      <div className="umap-deckgl-container">
        <DeckGL
          initialViewState={{
            longitude: centerX,
            latitude: centerY,
            zoom: Math.max(zoom, -5),
            minZoom: -10,
            maxZoom: 10,
          }}
          controller={true}
          layers={layers}
          getTooltip={({ object }: { object: UmapPoint | null }) => {
            if (!object) return null;
            return {
              html: `<div><b>Cell:</b> ${object.cell_id}<br/><b>Cluster:</b> ${object.cluster}</div>`,
              style: { backgroundColor: 'white', padding: '8px', borderRadius: '4px' },
            };
          }}
        />
      </div>
      <div className="legend">
        {clusters.map(c => (
          <span key={c} className="legend-item" style={{ borderColor: `rgb(${colorMap[c]?.join(',') || '0,0,0'})` }}>
            Cluster {c}
          </span>
        ))}
      </div>
    </div>
  );
}

export default UmapViewer;
