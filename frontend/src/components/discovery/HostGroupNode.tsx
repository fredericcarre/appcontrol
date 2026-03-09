import { memo, useState, useEffect } from 'react';
import type { NodeProps } from '@xyflow/react';
import { Server, Network } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { HostGroupNodeData } from './TopologyMap.types';

function HostGroupNodeInner({ data }: NodeProps & { data: HostGroupNodeData }) {
  const isAgentConnected = data.agentConnected ?? true;
  const isGatewayConnected = data.gatewayConnected ?? true;

  // Track if node is newly rendered for entrance animation
  const [isEntering, setIsEntering] = useState(true);
  useEffect(() => {
    const timer = setTimeout(() => setIsEntering(false), 400);
    return () => clearTimeout(timer);
  }, []);

  return (
    <div className={cn(
      "w-full h-full rounded-xl border-2 border-dashed border-slate-300 bg-slate-50/60 backdrop-blur-sm",
      isEntering && "discovery-host-entering"
    )}>
      <div className="flex items-center gap-2 px-3 py-2 border-b border-slate-200/80">
        <div className="flex items-center justify-center w-6 h-6 rounded-md bg-slate-200/80">
          <Server className="h-3.5 w-3.5 text-slate-600" />
        </div>
        <div className="flex-1 min-w-0">
          <span className="font-semibold text-sm text-slate-700 truncate block">{data.hostname}</span>
          {data.gatewayName && (
            <div className="flex items-center gap-1 text-[10px] text-slate-500">
              <Network className="h-2.5 w-2.5" />
              <span>{data.gatewayName}</span>
              {data.gatewayZone && <span className="text-slate-400">({data.gatewayZone})</span>}
            </div>
          )}
        </div>
        <div className="flex items-center gap-1.5">
          <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
            {data.serviceCount} {data.serviceCount === 1 ? 'service' : 'services'}
          </Badge>
          <div className="flex items-center gap-0.5">
            <div
              className={`w-2 h-2 rounded-full ${isGatewayConnected ? 'bg-blue-400' : 'bg-slate-300'}`}
              title={isGatewayConnected ? 'Gateway connected' : 'Gateway disconnected'}
            />
            <div
              className={`w-2 h-2 rounded-full ${isAgentConnected ? 'bg-emerald-500' : 'bg-slate-400'} ${isAgentConnected ? 'animate-pulse' : ''}`}
              title={isAgentConnected ? 'Agent connected' : 'Agent disconnected'}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

export const HostGroupNode = memo(HostGroupNodeInner);
