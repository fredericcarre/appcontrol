import { memo, useCallback, useState, useEffect } from 'react';
import { Handle, Position } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import { cn } from '@/lib/utils';
import { COMPONENT_TYPE_ICONS, TECHNOLOGY_ICONS, type ComponentType } from '@/lib/colors';
import {
  Database, Layers, Server, Globe, Cog, Clock, Box,
  Search, Calendar, ArrowLeftRight, Shield, Network,
  Workflow, Zap, Container, Folder, Puzzle, ShieldCheck, HelpCircle, Settings2,
} from 'lucide-react';
import type { ServiceNodeData } from './TopologyMap.types';
import { classifyConfidence } from './confidence';

const iconMap: Record<string, React.ComponentType<{ className?: string; style?: React.CSSProperties }>> = {
  Database, Layers, Server, Globe, Cog, Clock, Box,
  Search, Calendar, ArrowLeftRight, Shield, Network,
  Workflow, Zap, Container, Folder, Puzzle,
};

const CONFIDENCE_COLORS: Record<string, string> = {
  high: 'bg-emerald-500',
  medium: 'bg-amber-400',
  low: 'bg-slate-300',
};

// Confidence indicator styles
const CONFIDENCE_INDICATOR: Record<string, { icon: React.ComponentType<{ className?: string }>; color: string; bg: string }> = {
  recognized: { icon: ShieldCheck, color: 'text-emerald-600', bg: 'bg-emerald-100' },
  likely: { icon: Cog, color: 'text-amber-600', bg: 'bg-amber-100' },
  unknown: { icon: HelpCircle, color: 'text-slate-500', bg: 'bg-slate-100' },
  system: { icon: Settings2, color: 'text-slate-400', bg: 'bg-slate-50' },
};

function ServiceNodeInner({ data, selected }: NodeProps & { data: ServiceNodeData }) {
  // Use technology_hint if available, otherwise fall back to componentType
  const techHint = data.service?.technology_hint;
  const techInfo = techHint?.icon ? TECHNOLOGY_ICONS[techHint.icon] : null;
  const typeInfo = techInfo || COMPONENT_TYPE_ICONS[data.componentType as ComponentType] || COMPONENT_TYPE_ICONS.service;
  const IconComponent = iconMap[typeInfo.icon] || Box;
  const confColor = CONFIDENCE_COLORS[data.commandConfidence] || CONFIDENCE_COLORS.low;

  // Use technology display name if available
  const displayLabel = techHint?.display_name || data.label;

  // Get service confidence level
  const confidence = data.service ? classifyConfidence(data.service) : 'unknown';
  const confidenceStyle = CONFIDENCE_INDICATOR[confidence];
  const ConfidenceIcon = confidenceStyle.icon;

  // Track if node is newly rendered for entrance animation
  const [isEntering, setIsEntering] = useState(true);
  useEffect(() => {
    const timer = setTimeout(() => setIsEntering(false), 500);
    return () => clearTimeout(timer);
  }, []);

  const handleToggle = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      data.onToggle(data.serviceIndex);
    },
    [data]
  );

  const handleClick = useCallback(() => {
    data.onSelect(data.serviceIndex);
  }, [data]);

  return (
    <div
      onClick={handleClick}
      className={cn(
        'rounded-lg shadow-md border-2 w-[200px] bg-white cursor-pointer transition-all',
        selected && 'ring-2 ring-primary ring-offset-1',
        data.highlighted && 'discovery-node-glow discovery-node-pulse',
        !data.enabled && 'opacity-40',
        isEntering && 'discovery-node-entering',
      )}
      style={{
        borderColor: typeInfo.color,
        borderLeftWidth: 4,
        animationDelay: isEntering ? `${data.serviceIndex * 50}ms` : undefined,
      }}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!bg-blue-400 !border-2 !border-blue-600 !w-3 !h-3 hover:!w-4 hover:!h-4 hover:!bg-blue-500 transition-all cursor-crosshair"
      />

      <div className="p-2.5">
        {/* Top row: checkbox + name + confidence indicators */}
        <div className="flex items-center gap-1.5 mb-1">
          <input
            type="checkbox"
            checked={data.enabled}
            onClick={handleToggle}
            onChange={() => {}}
            className="h-3.5 w-3.5 rounded border-gray-300 text-primary focus:ring-primary cursor-pointer"
          />
          <IconComponent className="h-4 w-4 flex-shrink-0" style={{ color: typeInfo.color }} />
          <span className="font-semibold text-xs truncate flex-1" title={displayLabel}>
            {displayLabel}
          </span>
          {/* Confidence indicator */}
          <div
            className={cn('w-4 h-4 rounded flex items-center justify-center flex-shrink-0', confidenceStyle.bg)}
            title={`Confidence: ${confidence}`}
          >
            <ConfidenceIcon className={cn('h-2.5 w-2.5', confidenceStyle.color)} />
          </div>
          {/* Command confidence dot */}
          <div
            className={cn('w-2 h-2 rounded-full flex-shrink-0', confColor)}
            title={`Command confidence: ${data.commandConfidence}`}
          />
        </div>

        {/* Process name */}
        <div className="text-[10px] text-muted-foreground truncate">{data.processName}</div>

        {/* Ports */}
        {data.ports.length > 0 && (
          <div className="flex flex-wrap gap-0.5 mt-1">
            {data.ports.slice(0, 4).map((p) => (
              <span
                key={p}
                className="text-[9px] font-mono bg-slate-100 text-slate-600 px-1 rounded"
              >
                :{p}
              </span>
            ))}
            {data.ports.length > 4 && (
              <span className="text-[9px] text-muted-foreground">+{data.ports.length - 4}</span>
            )}
          </div>
        )}
      </div>

      <Handle
        type="source"
        position={Position.Bottom}
        className="!bg-emerald-400 !border-2 !border-emerald-600 !w-3 !h-3 hover:!w-4 hover:!h-4 hover:!bg-emerald-500 transition-all cursor-crosshair"
      />
    </div>
  );
}

export const ServiceNode = memo(ServiceNodeInner);
