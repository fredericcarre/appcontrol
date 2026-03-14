import { memo, useMemo, useState } from 'react';
import { getSmoothStepPath, EdgeLabelRenderer, type EdgeProps } from '@xyflow/react';
import { X } from 'lucide-react';
import { TECHNOLOGY_COLORS } from '@/lib/colors';
import { useDiscoveryStore } from '@/stores/discovery';
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
  const [isHovered, setIsHovered] = useState(false);
  const { ignoreDependency, removeManualDependency, dependencyMode } = useDiscoveryStore();

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
  const isManual = data?.inferredVia === 'manual';

  const handleRemove = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (data?.fromServiceIndex !== undefined && data?.toServiceIndex !== undefined) {
      if (isManual) {
        removeManualDependency(data.fromServiceIndex, data.toServiceIndex);
      } else {
        ignoreDependency(data.fromServiceIndex, data.toServiceIndex);
      }
    }
  };

  // Generate unique particle IDs for animation offsets
  const particleIds = useMemo(() => [0, 1, 2].map((i) => `${id}-particle-${i}`), [id]);

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
        id={`${id}-path`}
        d={edgePath}
        fill="none"
        stroke={color}
        strokeWidth={2}
        strokeOpacity={0.6}
        markerEnd={markerEnd}
      />

      {/* Flowing particles along the edge */}
      {particleIds.map((particleId, i) => (
        <circle
          key={particleId}
          r={3}
          fill={color}
          opacity={0.8}
        >
          <animateMotion
            dur={`${2 + i * 0.3}s`}
            repeatCount="indefinite"
            begin={`${i * 0.7}s`}
          >
            <mpath href={`#${id}-path`} />
          </animateMotion>
          <animate
            attributeName="opacity"
            values="0;0.8;0.8;0"
            dur={`${2 + i * 0.3}s`}
            repeatCount="indefinite"
            begin={`${i * 0.7}s`}
          />
        </circle>
      ))}

      {/* Label */}
      <EdgeLabelRenderer>
        <div
          style={{
            position: 'absolute',
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
            pointerEvents: 'all',
          }}
          className="nodrag nopan"
          onMouseEnter={() => setIsHovered(true)}
          onMouseLeave={() => setIsHovered(false)}
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
            {isManual && (
              <span className="text-[8px] bg-blue-100 text-blue-700 px-1 rounded">
                manual
              </span>
            )}
            {/* Remove button - shows on hover or in delete mode */}
            {(isHovered || dependencyMode === 'delete') && (
              <button
                onClick={handleRemove}
                className="ml-0.5 p-0.5 rounded hover:bg-red-100 text-slate-400 hover:text-red-500 transition-colors"
                title={isManual ? 'Remove manual dependency' : 'Ignore this dependency'}
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>
        </div>
      </EdgeLabelRenderer>
    </>
  );
}

export const DependencyEdge = memo(DependencyEdgeInner);
