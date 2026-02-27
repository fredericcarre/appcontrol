import { memo } from 'react';
import { getSmoothStepPath, EdgeLabelRenderer, type EdgeProps } from '@xyflow/react';
import { TECHNOLOGY_COLORS } from '@/lib/colors';
import type { DependencyEdgeData } from './TopologyMap.types';

function DependencyEdgeInner({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  markerEnd,
}: EdgeProps & { data: DependencyEdgeData }) {
  const [edgePath, labelX, labelY] = getSmoothStepPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    borderRadius: 8,
  });

  const tech = data?.technology || 'default';
  const color = TECHNOLOGY_COLORS[tech] || TECHNOLOGY_COLORS.default;
  const isConfig = data?.inferredVia === 'config_file';

  return (
    <>
      {/* Background glow */}
      <path
        d={edgePath}
        fill="none"
        stroke={color}
        strokeWidth={6}
        strokeOpacity={0.1}
      />
      {/* Main edge */}
      <path
        id={id}
        d={edgePath}
        fill="none"
        stroke={color}
        strokeWidth={2}
        className="discovery-edge-animated"
        markerEnd={markerEnd}
      />
      {/* Label */}
      <EdgeLabelRenderer>
        <div
          style={{
            position: 'absolute',
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
            pointerEvents: 'all',
          }}
          className="nodrag nopan"
        >
          <div className="flex items-center gap-1 bg-white/90 backdrop-blur-sm border border-slate-200 rounded px-1.5 py-0.5 shadow-sm">
            {data?.technology && (
              <span
                className="text-[10px] font-medium"
                style={{ color }}
              >
                {data.technology}
              </span>
            )}
            <span className="text-[9px] font-mono text-slate-500">
              :{data?.port}
            </span>
            {isConfig && (
              <span className="text-[8px] bg-emerald-100 text-emerald-700 px-1 rounded">
                cfg
              </span>
            )}
          </div>
        </div>
      </EdgeLabelRenderer>
    </>
  );
}

export const DependencyEdge = memo(DependencyEdgeInner);
