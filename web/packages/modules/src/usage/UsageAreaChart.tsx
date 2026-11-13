import React, { useState, useId } from "react";
import { UsageDataPoint } from "./types";

interface UsageAreaChartProps {
  data: UsageDataPoint[];
  dataKey: keyof UsageDataPoint;
  secondaryKey?: keyof UsageDataPoint;
  unit?: string;
  color?: "primary" | "emerald" | "amber" | "cyan" | "indigo" | "rose";
  height?: number;
  maxVal?: number;
}

export function UsageAreaChart({
  data,
  dataKey,
  secondaryKey,
  unit = "",
  color = "primary",
  height = 200,
  maxVal,
}: UsageAreaChartProps) {
  const gradientId = useId();
  const secondaryGradientId = useId();
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  if (!data || data.length === 0) {
    return (
      <div
        style={{ height }}
        className="w-full flex items-center justify-center text-xs text-muted-foreground border border-dashed rounded-lg"
      >
        No chart data available
      </div>
    );
  }

  // Determine values
  const primaryValues = data.map((d) => Number(d[dataKey]) || 0);
  const secondaryValues = secondaryKey ? data.map((d) => Number(d[secondaryKey]) || 0) : [];

  const rawMax = Math.max(
    ...primaryValues,
    ...(secondaryValues.length > 0 ? secondaryValues : [0]),
    maxVal ?? 1
  );
  // Add 10% ceiling headroom for aesthetics
  const maxY = Math.ceil(rawMax * 1.15) || 10;
  const minY = 0;

  // Chart coordinates mapping (Width: 600, Height: 180, Padding: top 10, bottom 25, left 45, right 15)
  const svgWidth = 600;
  const svgHeight = 180;
  const padLeft = 45;
  const padRight = 15;
  const padTop = 15;
  const padBottom = 25;

  const chartW = svgWidth - padLeft - padRight;
  const chartH = svgHeight - padTop - padBottom;

  const getX = (index: number) => {
    if (data.length <= 1) return padLeft + chartW / 2;
    return padLeft + (index / (data.length - 1)) * chartW;
  };

  const getY = (val: number) => {
    const clamped = Math.max(minY, Math.min(maxY, val));
    return padTop + chartH - (clamped / (maxY - minY)) * chartH;
  };

  // Build SVG path
  const buildPath = (values: number[]) => {
    if (values.length === 0) return "";
    return values
      .map((val, idx) => {
        const x = getX(idx);
        const y = getY(val);
        return `${idx === 0 ? "M" : "L"} ${x.toFixed(1)} ${y.toFixed(1)}`;
      })
      .join(" ");
  };

  const buildAreaPath = (values: number[]) => {
    if (values.length === 0) return "";
    const linePath = buildPath(values);
    const lastX = getX(values.length - 1);
    const firstX = getX(0);
    const bottomY = padTop + chartH;
    return `${linePath} L ${lastX.toFixed(1)} ${bottomY.toFixed(1)} L ${firstX.toFixed(1)} ${bottomY.toFixed(1)} Z`;
  };

  const linePath = buildPath(primaryValues);
  const areaPath = buildAreaPath(primaryValues);

  const secondaryLinePath = secondaryKey ? buildPath(secondaryValues) : "";
  const secondaryAreaPath = secondaryKey ? buildAreaPath(secondaryValues) : "";

  // Color scheme definitions (both light and dark theme safe)
  const colorMap = {
    primary: {
      stroke: "stroke-blue-600 dark:stroke-blue-400",
      fill: "#3b82f6",
      badge: "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20",
    },
    emerald: {
      stroke: "stroke-emerald-600 dark:stroke-emerald-400",
      fill: "#10b981",
      badge: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
    },
    amber: {
      stroke: "stroke-amber-600 dark:stroke-amber-400",
      fill: "#f59e0b",
      badge: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
    },
    cyan: {
      stroke: "stroke-cyan-600 dark:stroke-cyan-400",
      fill: "#06b6d4",
      badge: "bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border-cyan-500/20",
    },
    indigo: {
      stroke: "stroke-indigo-600 dark:stroke-indigo-400",
      fill: "#6366f1",
      badge: "bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 border-indigo-500/20",
    },
    rose: {
      stroke: "stroke-rose-600 dark:stroke-rose-400",
      fill: "#f43f5e",
      badge: "bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/20",
    },
  };

  const themeStyle = colorMap[color] || colorMap.primary;

  // Grid levels (0%, 25%, 50%, 75%, 100%)
  const yTicks = [0, 0.25, 0.5, 0.75, 1].map((pct) => ({
    val: Math.round(minY + pct * (maxY - minY)),
    y: padTop + chartH - pct * chartH,
  }));

  // X ticks (5 evenly spaced)
  const xStep = Math.max(1, Math.floor(data.length / 5));
  const xTicks = data
    .map((d, idx) => ({ d, idx }))
    .filter(({ idx }) => idx % xStep === 0 || idx === data.length - 1);

  const handleMouseMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const clientX = e.clientX - rect.left;
    const ratio = Math.max(0, Math.min(1, (clientX - (padLeft / svgWidth) * rect.width) / ((chartW / svgWidth) * rect.width)));
    const index = Math.round(ratio * (data.length - 1));
    if (index >= 0 && index < data.length) {
      setHoveredIndex(index);
    }
  };

  const handleMouseLeave = () => {
    setHoveredIndex(null);
  };

  const hoveredItem = hoveredIndex !== null ? data[hoveredIndex] : null;

  return (
    <div className="relative w-full select-none" data-testid="usage-area-chart">
      {/* Tooltip Card Overlay */}
      {hoveredItem && hoveredIndex !== null && (
        <div
          style={{
            left: `${((getX(hoveredIndex) / svgWidth) * 100).toFixed(1)}%`,
          }}
          className="absolute top-2 -translate-x-1/2 pointer-events-none z-10 flex flex-col items-center"
        >
          <div className="bg-popover/95 text-popover-foreground border shadow-lg backdrop-blur-sm rounded-md px-2.5 py-1.5 text-xs whitespace-nowrap">
            <div className="text-[10px] text-muted-foreground font-mono">
              {hoveredItem.label || hoveredItem.timestamp}
            </div>
            <div className="font-semibold text-foreground flex items-center gap-1.5 mt-0.5">
              <span className="w-2 h-2 rounded-full" style={{ backgroundColor: themeStyle.fill }}></span>
              <span>
                {Number(hoveredItem[dataKey]).toLocaleString()} {unit}
              </span>
              {secondaryKey && (
                <span className="text-muted-foreground font-normal ml-1">
                  / Out: {Number(hoveredItem[secondaryKey]).toLocaleString()} {unit}
                </span>
              )}
            </div>
          </div>
        </div>
      )}

      {/* SVG Canvas */}
      <svg
        viewBox={`0 0 ${svgWidth} ${svgHeight}`}
        className="w-full h-full overflow-visible"
        style={{ height }}
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
      >
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={themeStyle.fill} stopOpacity="0.35" />
            <stop offset="100%" stopColor={themeStyle.fill} stopOpacity="0.02" />
          </linearGradient>

          {secondaryKey && (
            <linearGradient id={secondaryGradientId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#8b5cf6" stopOpacity="0.3" />
              <stop offset="100%" stopColor="#8b5cf6" stopOpacity="0.02" />
            </linearGradient>
          )}
        </defs>

        {/* Horizontal Grid lines */}
        {yTicks.map(({ val, y }, idx) => (
          <g key={idx}>
            <line
              x1={padLeft}
              y1={y}
              x2={svgWidth - padRight}
              y2={y}
              className="stroke-zinc-200 dark:stroke-zinc-800"
              strokeDasharray="3 3"
              strokeWidth="1"
            />
            <text
              x={padLeft - 8}
              y={y + 3}
              textAnchor="end"
              className="fill-zinc-500 dark:fill-zinc-400 font-mono text-[10px]"
            >
              {val}
              {idx === yTicks.length - 1 ? unit : ""}
            </text>
          </g>
        ))}

        {/* X-Axis Time Labels */}
        {xTicks.map(({ d, idx }) => (
          <text
            key={idx}
            x={getX(idx)}
            y={svgHeight - 6}
            textAnchor="middle"
            className="fill-zinc-500 dark:fill-zinc-400 font-mono text-[10px]"
          >
            {d.label}
          </text>
        ))}

        {/* Secondary Area & Line (if dual series, e.g. outbound network) */}
        {secondaryKey && (
          <>
            <path d={secondaryAreaPath} fill={`url(#${secondaryGradientId})`} />
            <path
              d={secondaryLinePath}
              fill="none"
              stroke="#8b5cf6"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </>
        )}

        {/* Primary Area & Line */}
        <path d={areaPath} fill={`url(#${gradientId})`} />
        <path
          d={linePath}
          fill="none"
          className={themeStyle.stroke}
          strokeWidth="2.2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />

        {/* Active hover crosshair and point */}
        {hoveredIndex !== null && (
          <g>
            <line
              x1={getX(hoveredIndex)}
              y1={padTop}
              x2={getX(hoveredIndex)}
              y2={padTop + chartH}
              className="stroke-zinc-400 dark:stroke-zinc-600"
              strokeDasharray="2 2"
              strokeWidth="1.5"
            />
            <circle
              cx={getX(hoveredIndex)}
              cy={getY(primaryValues[hoveredIndex])}
              r="4.5"
              fill={themeStyle.fill}
              className="stroke-white dark:stroke-zinc-950 stroke-2"
            />
          </g>
        )}
      </svg>
    </div>
  );
}
