import { memo, useCallback } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { Server, Play, Square, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { STATE_COLORS, ComponentState } from '@/lib/colors';

// Compact node used to render a fan-out cluster member on the map when the
// user toggles the "explode members" view. Smaller than ComponentNode and
// has no edit affordances — members are managed from the parent's panel.
export interface MemberNodeData {
  hostname: string;
  state: ComponentState;
  isEnabled: boolean;
  agentHostname?: string | null;
  // Lit when the operator clicks Start/Stop on the parent's panel
  onStart?: (memberId: string) => void;
  onStop?: (memberId: string) => void;
  [key: string]: unknown;
}

function MemberNodeInner({ id, data }: NodeProps & { data: MemberNodeData }) {
  const stateStyle = STATE_COLORS[data.state] || STATE_COLORS.UNKNOWN;
  const handleStart = useCallback(() => data.onStart?.(id), [data, id]);
  const handleStop = useCallback(() => data.onStop?.(id), [data, id]);

  const isTransitioning = data.state === 'STARTING' || data.state === 'STOPPING';

  return (
    <div className="relative">
      <Handle type="target" position={Position.Top} className="!bg-gray-300 !w-1.5 !h-1.5" />

      <div
        className={cn(
          'rounded-md border min-w-[140px] px-2 py-1.5 text-xs shadow-sm',
          isTransitioning && 'animate-state-pulse',
          !data.isEnabled && 'opacity-50',
        )}
        style={{
          backgroundColor: stateStyle.bg,
          borderColor: stateStyle.border,
          borderStyle: data.state === 'UNKNOWN' ? 'dashed' : 'solid',
        }}
      >
        <div className="flex items-center gap-1.5 mb-0.5">
          <Server className="h-3 w-3" style={{ color: stateStyle.border }} />
          <span className="font-mono truncate flex-1" title={data.hostname}>
            {data.hostname}
          </span>
          {!data.isEnabled && (
            <AlertTriangle className="h-3 w-3 text-amber-500" aria-label="disabled" />
          )}
        </div>
        <div className="flex items-center justify-between">
          <span className="text-[9px]" style={{ color: stateStyle.border }}>
            {data.state}
          </span>
          <div className="flex gap-0.5">
            {data.onStart && (
              <button
                onClick={handleStart}
                className="p-0.5 rounded hover:bg-white/60"
                title={`Start ${data.hostname}`}
                aria-label={`Start ${data.hostname}`}
              >
                <Play className="h-2.5 w-2.5 text-green-600" />
              </button>
            )}
            {data.onStop && (
              <button
                onClick={handleStop}
                className="p-0.5 rounded hover:bg-white/60"
                title={`Stop ${data.hostname}`}
                aria-label={`Stop ${data.hostname}`}
              >
                <Square className="h-2.5 w-2.5 text-red-600" />
              </button>
            )}
          </div>
        </div>
        {data.agentHostname && (
          <div className="text-[9px] text-muted-foreground truncate mt-0.5" title={data.agentHostname}>
            {data.agentHostname}
          </div>
        )}
      </div>

      <Handle type="source" position={Position.Bottom} className="!bg-gray-300 !w-1.5 !h-1.5" />
    </div>
  );
}

export const MemberNode = memo(MemberNodeInner);
