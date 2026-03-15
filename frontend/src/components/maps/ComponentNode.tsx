import { memo, useCallback, useState } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { cn } from '@/lib/utils';
import { STATE_COLORS, COMPONENT_TYPE_ICONS, ComponentState, ComponentType } from '@/lib/colors';
import {
  Database, Layers, Server, Globe, Cog, Clock, Box,
  Play, Square, RotateCcw, Search, Skull, GitBranch, Wrench,
  Shield, Cloud, HardDrive, Cpu, Network, FileText, Zap,
  ExternalLink, ArrowUp, ArrowDown, WifiOff, Unplug, Radio,
  BarChart3,
} from 'lucide-react';
import { MetricsDisplay, MetricWidget } from './MetricsDisplay';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

const iconMap: Record<string, React.ComponentType<{ className?: string; style?: React.CSSProperties }>> = {
  Database, Layers, Server, Globe, Cog, Clock, Box,
  Shield, Cloud, HardDrive, Cpu, Network, FileText, Zap,
  database: Database, layers: Layers, server: Server, globe: Globe,
  cog: Cog, clock: Clock, box: Box, shield: Shield, cloud: Cloud,
  'hard-drive': HardDrive, cpu: Cpu, network: Network,
  'file-text': FileText, zap: Zap,
};

interface ComponentNodeData {
  label: string;
  displayName?: string;
  description?: string;
  icon?: string;
  groupColor?: string;
  links?: Array<{ label: string; url: string }>;
  state: ComponentState;
  componentType: ComponentType;
  host: string;
  isErrorBranch?: boolean;
  highlightType?: 'none' | 'selected' | 'dependency' | 'dependent' | 'impact' | 'edge_endpoint' | 'infra';
  highlightColor?: string;
  // Cluster configuration
  clusterSize?: number | null;
  clusterNodes?: string[] | null;
  // Connectivity status
  connectivityStatus?: 'connected' | 'agent_disconnected' | 'gateway_disconnected' | 'no_agent';
  agentHostname?: string;
  agentId?: string;
  gatewayId?: string;
  // Metrics from check command output
  metrics?: Record<string, unknown> | null;
  metricsWidgets?: MetricWidget[];
  // Callbacks
  onStart?: (id: string) => void;
  onStop?: (id: string) => void;
  onRestart?: (id: string) => void;
  onDiagnose?: (id: string) => void;
  onForceStop?: (id: string) => void;
  onStartWithDeps?: (id: string) => void;
  onRepair?: (id: string) => void;
  [key: string]: unknown;
}

function ComponentNodeInner({ id, data, selected }: NodeProps & { data: ComponentNodeData }) {
  const stateStyle = STATE_COLORS[data.state] || STATE_COLORS.UNKNOWN;
  const typeInfo = COMPONENT_TYPE_ICONS[data.componentType] || COMPONENT_TYPE_ICONS.custom;

  const IconComponent = (data.icon && iconMap[data.icon]) || iconMap[typeInfo.icon] || Box;

  const handleStart = useCallback(() => data.onStart?.(id), [data, id]);
  const handleStop = useCallback(() => data.onStop?.(id), [data, id]);
  const handleRestart = useCallback(() => data.onRestart?.(id), [data, id]);
  const handleDiagnose = useCallback(() => data.onDiagnose?.(id), [data, id]);
  const handleForceStop = useCallback(() => data.onForceStop?.(id), [data, id]);
  const handleStartWithDeps = useCallback(() => data.onStartWithDeps?.(id), [data, id]);
  const handleRepair = useCallback(() => data.onRepair?.(id), [data, id]);

  const isTransitioning = data.state === 'STARTING' || data.state === 'STOPPING';
  const displayLabel = data.displayName || data.label;

  // Cluster support
  const isCluster = data.clusterSize && data.clusterSize >= 2;
  const stackCount = Math.min(data.clusterSize || 1, 3); // Max 3 visible stacked cards

  // Connectivity status
  const isDisconnected = data.connectivityStatus === 'agent_disconnected' ||
                         data.connectivityStatus === 'gateway_disconnected' ||
                         data.connectivityStatus === 'no_agent';

  const isHighlighted = data.highlightType && data.highlightType !== 'none';
  const isImpactHighlight = data.highlightType === 'impact';

  // Check if metrics exist (filter out _widget hint keys)
  const hasMetrics = data.metrics && Object.keys(data.metrics).some(k => !k.endsWith('_widget'));
  const metricsCount = data.metrics
    ? Object.keys(data.metrics).filter(k => !k.endsWith('_widget')).length
    : 0;

  // Determine border color (use string to allow dynamic colors)
  let borderColor: string = stateStyle.border;
  if (data.isErrorBranch) {
    borderColor = '#FF6B8A';
  } else if (isHighlighted && data.highlightColor) {
    borderColor = data.highlightColor;
  }

  // Determine background color (use string to allow dynamic colors)
  let bgColor: string = stateStyle.bg;
  if (data.isErrorBranch) {
    bgColor = '#FFE0E6';
  } else if (isHighlighted && data.highlightColor) {
    // Lighten the highlight color for background
    bgColor = `${data.highlightColor}15`;
  }

  // Common card styles for main and stacked cards
  const cardBaseClasses = 'rounded-lg border-2 min-w-[180px]';

  return (
    <div className="relative">
      {/* Stacked cards behind for cluster effect */}
      {isCluster && stackCount >= 3 && (
        <div
          className={cn(cardBaseClasses, 'absolute inset-0')}
          style={{
            backgroundColor: bgColor,
            borderColor: borderColor,
            borderStyle: data.state === 'UNKNOWN' ? 'dashed' : 'solid',
            transform: 'translate(6px, 6px)',
            opacity: 0.4,
            zIndex: -2,
          }}
        />
      )}
      {isCluster && stackCount >= 2 && (
        <div
          className={cn(cardBaseClasses, 'absolute inset-0')}
          style={{
            backgroundColor: bgColor,
            borderColor: borderColor,
            borderStyle: data.state === 'UNKNOWN' ? 'dashed' : 'solid',
            transform: 'translate(3px, 3px)',
            opacity: 0.6,
            zIndex: -1,
          }}
        />
      )}

      {/* Main card */}
      <div
        className={cn(
          cardBaseClasses,
          'shadow-md transition-all relative',
          selected && !isHighlighted && 'ring-2 ring-ring ring-offset-2',
          isTransitioning && 'animate-state-pulse',
          isImpactHighlight && 'ring-4 ring-offset-2 animate-pulse',
          isHighlighted && !isImpactHighlight && 'ring-2 ring-offset-1',
        )}
        style={{
          backgroundColor: bgColor,
          borderColor: borderColor,
          borderStyle: data.state === 'UNKNOWN' ? 'dashed' : 'solid',
          borderLeftWidth: data.groupColor ? 4 : undefined,
          borderLeftColor: data.groupColor || undefined,
          boxShadow: isHighlighted ? `0 0 15px ${data.highlightColor}50` : undefined,
          // @ts-expect-error CSS variable for ring color
          '--tw-ring-color': isHighlighted ? data.highlightColor : undefined,
        }}
      >
      {/* Source at top: sends edges to bases above */}
      <Handle type="source" position={Position.Top} className="!bg-gray-400 !w-2 !h-2" />

      {/* Branch indicator badge */}
      {isHighlighted && !isImpactHighlight && (
        <div
          className="absolute -top-2 -right-2 w-5 h-5 rounded-full flex items-center justify-center text-white text-[10px]"
          style={{ backgroundColor: data.highlightColor }}
          title={
            data.highlightType === 'dependency' ? 'Dependency (upstream)' :
            data.highlightType === 'dependent' ? 'Dependent (downstream)' :
            'Selected'
          }
        >
          {data.highlightType === 'dependency' && <ArrowUp className="w-3 h-3" />}
          {data.highlightType === 'dependent' && <ArrowDown className="w-3 h-3" />}
        </div>
      )}

      <div className="p-3">
        <div className="flex items-center gap-2 mb-1">
          <IconComponent className="h-4 w-4" style={{ color: typeInfo.color }} />
          <span className="font-semibold text-sm truncate flex-1" title={data.description || undefined}>
            {displayLabel}
          </span>
          {/* Cluster badge */}
          {isCluster && (
            <span
              className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-slate-200 text-slate-700"
              title={
                data.clusterNodes && data.clusterNodes.length > 0
                  ? `Cluster nodes: ${data.clusterNodes.join(', ')}`
                  : `${data.clusterSize} nodes`
              }
            >
              x{data.clusterSize}
            </span>
          )}
          {/* Metrics indicator badge - always visible when metrics exist */}
          {hasMetrics && (
            <Tooltip delayDuration={200}>
              <TooltipTrigger asChild>
                <span
                  className="inline-flex items-center gap-0.5 text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-indigo-500 text-white shadow-sm cursor-help"
                >
                  <BarChart3 className="h-3 w-3" />
                  <span>{metricsCount}</span>
                </span>
              </TooltipTrigger>
              <TooltipContent side="right" className="max-w-[280px] p-0">
                <div className="p-2 space-y-2">
                  <div className="text-xs font-semibold border-b pb-1">Metrics Preview</div>
                  <MetricsTooltipContent metrics={data.metrics!} />
                </div>
              </TooltipContent>
            </Tooltip>
          )}
        </div>

        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground truncate max-w-[100px]" title={data.host}>
            {data.agentHostname || data.host}
          </span>
          <div className="flex items-center gap-1">
            {isDisconnected && (
              <span
                className="inline-flex items-center gap-0.5 text-[10px] px-1 py-0.5 rounded bg-red-100 text-red-700"
                title={
                  data.connectivityStatus === 'no_agent' ? 'No agent assigned' :
                  data.connectivityStatus === 'gateway_disconnected' ? 'Gateway disconnected' :
                  'Agent disconnected'
                }
              >
                {data.connectivityStatus === 'gateway_disconnected' ? (
                  <Unplug className="h-2.5 w-2.5" />
                ) : (
                  <WifiOff className="h-2.5 w-2.5" />
                )}
              </span>
            )}
            <span
              className="text-xs font-medium px-1.5 py-0.5 rounded"
              style={{ color: stateStyle.border }}
            >
              {data.state}
            </span>
          </div>
        </div>

        {/* Metrics display (compact mode, always visible when available) */}
        {data.metrics && Object.keys(data.metrics).length > 0 && (
          <div className="mt-2 pt-2 border-t border-gray-200">
            <MetricsDisplay
              metrics={data.metrics}
              widgets={data.metricsWidgets}
              compact={true}
            />
          </div>
        )}

        {selected && (
          <>
            {/* Infrastructure info */}
            {(data.agentHostname || data.gatewayId) && (
              <div className="flex flex-wrap gap-1 mt-2 pt-2 border-t border-gray-200">
                {data.agentHostname && (
                  <span
                    className={`inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded ${
                      data.connectivityStatus === 'connected'
                        ? 'bg-blue-100 text-blue-700'
                        : 'bg-orange-100 text-orange-700'
                    }`}
                    title={`Agent: ${data.agentHostname}`}
                  >
                    <Server className="h-2.5 w-2.5" />
                    <span className="max-w-[80px] truncate">{data.agentHostname}</span>
                  </span>
                )}
                {data.gatewayId && (
                  <span
                    className={`inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded ${
                      data.connectivityStatus !== 'gateway_disconnected'
                        ? 'bg-emerald-100 text-emerald-700'
                        : 'bg-red-100 text-red-700'
                    }`}
                    title={`Gateway: ${data.gatewayId.slice(0, 8)}...`}
                  >
                    <Radio className="h-2.5 w-2.5" />
                    <span>{data.gatewayId.slice(0, 6)}</span>
                  </span>
                )}
              </div>
            )}
            <div className="flex gap-1 mt-1.5 pt-1.5 border-t border-gray-200">
              <button onClick={handleStart} className="p-1 rounded hover:bg-white/50" title="Start">
                <Play className="h-3.5 w-3.5 text-green-600" />
              </button>
              <button onClick={handleStop} className="p-1 rounded hover:bg-white/50" title="Stop">
                <Square className="h-3.5 w-3.5 text-red-600" />
              </button>
              <button onClick={handleRestart} className="p-1 rounded hover:bg-white/50" title="Restart">
                <RotateCcw className="h-3.5 w-3.5 text-blue-600" />
              </button>
              <button onClick={handleRepair} className="p-1 rounded hover:bg-white/50" title="Repair (restart with dependents)">
                <Wrench className="h-3.5 w-3.5 text-orange-600" />
              </button>
              <button onClick={handleStartWithDeps} className="p-1 rounded hover:bg-white/50" title="Start with dependencies">
                <GitBranch className="h-3.5 w-3.5 text-teal-600" />
              </button>
              <button onClick={handleForceStop} className="p-1 rounded hover:bg-white/50" title="Force Kill (ignore dependencies)">
                <Skull className="h-3.5 w-3.5 text-red-800" />
              </button>
              <button onClick={handleDiagnose} className="p-1 rounded hover:bg-white/50" title="Diagnose">
                <Search className="h-3.5 w-3.5 text-purple-600" />
              </button>
            </div>
            {data.links && data.links.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-1.5">
                {data.links.map((link, i) => (
                  <a
                    key={i}
                    href={link.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-0.5 text-[10px] text-blue-600 hover:underline"
                  >
                    <ExternalLink className="h-2.5 w-2.5" />
                    {link.label}
                  </a>
                ))}
              </div>
            )}
          </>
        )}
      </div>

        {/* Target at bottom: receives edges from dependents below */}
        <Handle type="target" position={Position.Bottom} className="!bg-gray-400 !w-2 !h-2" />
      </div>
    </div>
  );
}

// Compact metrics preview for tooltip
function MetricsTooltipContent({ metrics }: { metrics: Record<string, unknown> }) {
  // Filter out widget hints and get first 6 key metrics
  const entries = Object.entries(metrics)
    .filter(([k]) => !k.endsWith('_widget'))
    .slice(0, 6);

  if (entries.length === 0) return <div className="text-xs text-muted-foreground">No metrics</div>;

  return (
    <div className="grid grid-cols-2 gap-x-3 gap-y-1 text-xs">
      {entries.map(([key, value]) => {
        const label = key.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
        let displayValue: string;

        if (typeof value === 'number') {
          displayValue = value >= 1000 ? `${(value / 1000).toFixed(1)}K` : String(value);
        } else if (Array.isArray(value)) {
          displayValue = `[${value.length} items]`;
        } else if (typeof value === 'object' && value !== null) {
          displayValue = `{${Object.keys(value).length} keys}`;
        } else {
          displayValue = String(value).slice(0, 20);
        }

        return (
          <div key={key} className="flex justify-between gap-2">
            <span className="text-muted-foreground truncate">{label}:</span>
            <span className="font-medium">{displayValue}</span>
          </div>
        );
      })}
      {Object.keys(metrics).filter(k => !k.endsWith('_widget')).length > 6 && (
        <div className="col-span-2 text-muted-foreground text-center mt-1">
          +{Object.keys(metrics).filter(k => !k.endsWith('_widget')).length - 6} more...
        </div>
      )}
    </div>
  );
}

export const ComponentNode = memo(ComponentNodeInner);
