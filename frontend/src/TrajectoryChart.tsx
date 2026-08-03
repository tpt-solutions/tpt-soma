import React, { useMemo, useState } from 'react';

export interface TrajectoryPoint {
  timestamp: string;
  value: number;
}

export interface TrajectoryBand {
  from: number;
  to: number;
  color: string;
  label: string;
}

interface TrajectoryChartProps {
  title: string;
  unit: string;
  points: TrajectoryPoint[];
  bands?: TrajectoryBand[];
  referenceRange?: [number, number];
  lineColor?: string;
}

const WIDTH = 720;
const HEIGHT = 280;
const MARGIN = { top: 16, right: 16, bottom: 28, left: 48 };

function TrajectoryChart({
  title,
  unit,
  points,
  bands,
  referenceRange,
  lineColor = '#2a78d6',
}: TrajectoryChartProps) {
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  const sorted = useMemo(
    () => [...points].sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()),
    [points],
  );

  if (sorted.length === 0) {
    return <p>No data to display.</p>;
  }

  const plotWidth = WIDTH - MARGIN.left - MARGIN.right;
  const plotHeight = HEIGHT - MARGIN.top - MARGIN.bottom;

  const times = sorted.map(p => new Date(p.timestamp).getTime());
  const values = sorted.map(p => p.value);

  const bandBounds = bands ? bands.flatMap(b => [b.from, b.to]) : [];
  const refBounds = referenceRange ? [referenceRange[0], referenceRange[1]] : [];
  const allValues = [...values, ...bandBounds, ...refBounds].filter(v => Number.isFinite(v));

  const tMin = Math.min(...times);
  const tMax = Math.max(...times);
  const vMin = Math.min(...allValues);
  const vMax = Math.max(...allValues);
  const vPad = (vMax - vMin) * 0.08 || 1;
  const yLow = vMin - vPad;
  const yHigh = vMax + vPad;

  const xScale = (t: number) => {
    if (tMax === tMin) return plotWidth / 2;
    return ((t - tMin) / (tMax - tMin)) * plotWidth;
  };
  const yScale = (v: number) => plotHeight - ((v - yLow) / (yHigh - yLow)) * plotHeight;

  const linePath = sorted
    .map((p, i) => {
      const x = xScale(new Date(p.timestamp).getTime());
      const y = yScale(p.value);
      return `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(' ');

  const yTicks = 4;
  const tickValues = Array.from({ length: yTicks + 1 }, (_, i) => yLow + ((yHigh - yLow) * i) / yTicks);

  const handleMove = (e: React.MouseEvent<SVGRectElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const relX = e.clientX - rect.left;
    const targetT = tMin + (relX / plotWidth) * (tMax - tMin);
    let nearest = 0;
    let bestDiff = Infinity;
    times.forEach((t, i) => {
      const diff = Math.abs(t - targetT);
      if (diff < bestDiff) {
        bestDiff = diff;
        nearest = i;
      }
    });
    setHoverIndex(nearest);
  };

  const hoverPoint = hoverIndex !== null ? sorted[hoverIndex] : null;

  return (
    <div className="trajectory-chart">
      <div className="trajectory-chart-header">
        <h4>{title}</h4>
        {referenceRange && (
          <span className="trajectory-chart-refrange">
            Reference: {referenceRange[0]}&ndash;{referenceRange[1]} {unit}
          </span>
        )}
      </div>

      <svg width="100%" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={title}>
        <g transform={`translate(${MARGIN.left}, ${MARGIN.top})`}>
          {bands?.map((band, i) => {
            const y1 = yScale(Math.min(band.to, yHigh));
            const y2 = yScale(Math.max(band.from, yLow));
            return (
              <rect
                key={i}
                x={0}
                y={Math.min(y1, y2)}
                width={plotWidth}
                height={Math.abs(y2 - y1)}
                fill={band.color}
                opacity={0.14}
              />
            );
          })}

          {referenceRange && (
            <rect
              x={0}
              y={yScale(referenceRange[1])}
              width={plotWidth}
              height={yScale(referenceRange[0]) - yScale(referenceRange[1])}
              fill="#0ca30c"
              opacity={0.1}
            />
          )}

          {tickValues.map((v, i) => (
            <g key={i}>
              <line
                x1={0}
                x2={plotWidth}
                y1={yScale(v)}
                y2={yScale(v)}
                stroke="#e1e0d9"
                strokeWidth={1}
              />
              <text x={-8} y={yScale(v)} dy="0.32em" textAnchor="end" fontSize={11} fill="#898781">
                {v.toFixed(0)}
              </text>
            </g>
          ))}

          <path d={linePath} fill="none" stroke={lineColor} strokeWidth={2} strokeLinejoin="round" />

          {sorted.map((p, i) => (
            <circle
              key={i}
              cx={xScale(new Date(p.timestamp).getTime())}
              cy={yScale(p.value)}
              r={hoverIndex === i ? 4 : 2.5}
              fill={lineColor}
            />
          ))}

          {hoverIndex !== null && (
            <line
              x1={xScale(times[hoverIndex])}
              x2={xScale(times[hoverIndex])}
              y1={0}
              y2={plotHeight}
              stroke="#898781"
              strokeWidth={1}
              strokeDasharray="3,3"
            />
          )}

          <rect
            x={0}
            y={0}
            width={plotWidth}
            height={plotHeight}
            fill="transparent"
            onMouseMove={handleMove}
            onMouseLeave={() => setHoverIndex(null)}
          />
        </g>
      </svg>

      {hoverPoint && (
        <div className="trajectory-chart-tooltip">
          <strong>{new Date(hoverPoint.timestamp).toLocaleString()}</strong>: {hoverPoint.value} {unit}
        </div>
      )}

      {bands && bands.length > 0 && (
        <div className="legend">
          {bands.map((b, i) => (
            <span key={i} className="legend-item" style={{ borderColor: b.color }}>
              {b.label}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export default TrajectoryChart;
