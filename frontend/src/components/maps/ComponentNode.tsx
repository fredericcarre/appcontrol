import { memo, useCallback } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { cn } from '@/lib/utils';
import { STATE_COLORS, COMPONENT_TYPE_ICONS, ComponentState, ComponentType } from '@/lib/colors';
import {
  Database, Layers, Server, Globe, Cog, Clock, Box,
  Play, Square, RotateCcw, Search,
} from 'lucide-react';

const iconMap: Record<string, React.ComponentType<{ className?: string; style?: React.CSSProperties }>> = {
  Database, Layers, Server, Globe, Cog, Clock, Box,
};

interface ComponentNodeData {
  label: string;
  state: ComponentState;
  componentType: ComponentType;
  host: string;
  isErrorBranch?: boolean;
  onStart?: (id: string) => void;
  onStop?: (id: string) => void;
  onRestart?: (id: string) => void;
  onDiagnose?: (id: string) => void;
  [key: string]: unknown;
}

function ComponentNodeInner({ id, data, selected }: NodeProps & { data: ComponentNodeData }) {
  const stateStyle = STATE_COLORS[data.state] || STATE_COLORS.UNKNOWN;
  const typeInfo = COMPONENT_TYPE_ICONS[data.componentType] || COMPONENT_TYPE_ICONS.custom;
  const IconComponent = iconMap[typeInfo.icon] || Box;

  const handleStart = useCallback(() => data.onStart?.(id), [data, id]);
  const handleStop = useCallback(() => data.onStop?.(id), [data, id]);
  const handleRestart = useCallback(() => data.onRestart?.(id), [data, id]);
  const handleDiagnose = useCallback(() => data.onDiagnose?.(id), [data, id]);

  const isTransitioning = data.state === 'STARTING' || data.state === 'STOPPING';

  return (
    <div
      className={cn(
        'rounded-lg shadow-md border-2 min-w-[180px] transition-all',
        selected && 'ring-2 ring-ring ring-offset-2',
        isTransitioning && 'animate-state-pulse',
      )}
      style={{
        backgroundColor: data.isErrorBranch ? '#FFE0E6' : stateStyle.bg,
        borderColor: data.isErrorBranch ? '#FF6B8A' : stateStyle.border,
        borderStyle: data.state === 'UNKNOWN' ? 'dashed' : 'solid',
      }}
    >
      <Handle type="target" position={Position.Top} className="!bg-gray-400 !w-2 !h-2" />

      <div className="p-3">
        <div className="flex items-center gap-2 mb-1">
          <IconComponent className="h-4 w-4" style={{ color: typeInfo.color }} />
          <span className="font-semibold text-sm truncate">{data.label}</span>
        </div>

        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">{data.host}</span>
          <span
            className="text-xs font-medium px-1.5 py-0.5 rounded"
            style={{ color: stateStyle.border }}
          >
            {data.state}
          </span>
        </div>

        {selected && (
          <div className="flex gap-1 mt-2 pt-2 border-t border-gray-200">
            <button onClick={handleStart} className="p-1 rounded hover:bg-white/50" title="Start">
              <Play className="h-3.5 w-3.5 text-green-600" />
            </button>
            <button onClick={handleStop} className="p-1 rounded hover:bg-white/50" title="Stop">
              <Square className="h-3.5 w-3.5 text-red-600" />
            </button>
            <button onClick={handleRestart} className="p-1 rounded hover:bg-white/50" title="Restart">
              <RotateCcw className="h-3.5 w-3.5 text-blue-600" />
            </button>
            <button onClick={handleDiagnose} className="p-1 rounded hover:bg-white/50" title="Diagnose">
              <Search className="h-3.5 w-3.5 text-purple-600" />
            </button>
          </div>
        )}
      </div>

      <Handle type="source" position={Position.Bottom} className="!bg-gray-400 !w-2 !h-2" />
    </div>
  );
}

export const ComponentNode = memo(ComponentNodeInner);
