import { memo, useCallback } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { Server, Play, Square, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { STATE_COLORS, ComponentState } from '@/lib/colors';

// Compact node used to render a fan-out cluster member on the map when the
// user toggles the "explode members" view. Smaller than ComponentNode and
// has no edit affordances — members are managed from the parent's panel.
//
// `compact` switches to a tighter render (110×40 with no agent line and no
// inline action buttons) for clusters with > 30 members, so a 200-node tier
// stays navigable. The full render (140×50 with start/stop buttons and an
// agent footer) is used for smaller demos like the 6-node JBoss tier.
//
// IMPORTANT: the React Flow `id` for a member node is prefixed with
// `member-` (see AppMap.tsx) to avoid colliding with regular component
// nodes in the same graph. The cluster_members.id UUID that start/stop
// actions need is therefore passed in `data.memberId`, NOT in `id`.
// Forgetting this broke the in-graph Start/Stop buttons in v1.18.0 — the
// per-member-action handler couldn't resolve the prefix back to a uuid
// and silently no-op'd.
export interface MemberNodeData {
  /** The cluster_members.id UUID — what start/stop dispatches against. */
  memberId: string;
  hostname: string;
  state: ComponentState;
  isEnabled: boolean;
  agentHostname?: string | null;
  compact?: boolean;
  // Lit when the operator clicks Start/Stop on the parent's panel
  onStart?: (memberId: string) => void;
  onStop?: (memberId: string) => void;
  [key: string]: unknown;
}

function MemberNodeInner({ data }: NodeProps & { data: MemberNodeData }) {
  const stateStyle = STATE_COLORS[data.state] || STATE_COLORS.UNKNOWN;
  const handleStart = useCallback(
    (e: React.MouseEvent) => {
      // React Flow swallows clicks on nodes for selection/drag — stopPropagation
      // here so an inline button click doesn't double-fire as a node click.
      e.stopPropagation();
      data.onStart?.(data.memberId);
    },
    [data],
  );
  const handleStop = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      data.onStop?.(data.memberId);
    },
    [data],
  );

  const isTransitioning = data.state === 'STARTING' || data.state === 'STOPPING';
  const compact = !!data.compact;

  // Compact tile (used for >30 members): hostname only, full-tile colour
  // by state, no inline buttons. Tooltip carries the state for context.
  // Operators wanting to act on a member at this scale go through the
  // Members tab in the side panel — the per-tile buttons would be too
  // small to reliably click anyway.
  if (compact) {
    return (
      <div className="relative">
        <Handle type="target" position={Position.Top} className="!bg-gray-300 !w-1 !h-1" />
        <div
          className={cn(
            'rounded-md border min-w-[110px] max-w-[110px] px-2 py-1 text-[11px] font-mono shadow-sm truncate',
            isTransitioning && 'animate-state-pulse',
            !data.isEnabled && 'opacity-50',
          )}
          style={{
            backgroundColor: stateStyle.bg,
            borderColor: stateStyle.border,
            borderStyle: data.state === 'UNKNOWN' ? 'dashed' : 'solid',
            color: stateStyle.border,
          }}
          title={`${data.hostname} — ${data.state}`}
        >
          {data.hostname}
        </div>
        <Handle type="source" position={Position.Bottom} className="!bg-gray-300 !w-1 !h-1" />
      </div>
    );
  }

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
                className="p-0.5 rounded hover:bg-white/60 nodrag"
                title={`Start ${data.hostname}`}
                aria-label={`Start ${data.hostname}`}
              >
                <Play className="h-2.5 w-2.5 text-green-600" />
              </button>
            )}
            {data.onStop && (
              <button
                onClick={handleStop}
                className="p-0.5 rounded hover:bg-white/60 nodrag"
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
