import { memo } from 'react';
import { getSmoothStepPath, EdgeLabelRenderer, type EdgeProps } from '@xyflow/react';
import type { UnresolvedEdgeData } from './TopologyMap.types';

function UnresolvedEdgeInner({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
}: EdgeProps & { data: UnresolvedEdgeData }) {
  const [edgePath, labelX, labelY] = getSmoothStepPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    borderRadius: 8,
  });

  return (
    <>
      <path
        id={id}
        d={edgePath}
        fill="none"
        stroke="#94a3b8"
        strokeWidth={1.5}
        strokeDasharray="8 4"
        strokeOpacity={0.6}
      />
      <EdgeLabelRenderer>
        <div
          style={{
            position: 'absolute',
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
            pointerEvents: 'all',
          }}
          className="nodrag nopan"
        >
          <span className="text-[9px] font-mono text-slate-400 bg-white/80 rounded px-1">
            :{data?.port}
          </span>
        </div>
      </EdgeLabelRenderer>
    </>
  );
}

export const UnresolvedEdge = memo(UnresolvedEdgeInner);
