import { memo } from 'react';
import type { NodeProps } from '@xyflow/react';
import { Server } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import type { HostGroupNodeData } from './TopologyMap.types';

function HostGroupNodeInner({ data }: NodeProps & { data: HostGroupNodeData }) {
  return (
    <div className="w-full h-full rounded-xl border-2 border-dashed border-slate-300 bg-slate-50/60 backdrop-blur-sm">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-slate-200/80">
        <div className="flex items-center justify-center w-6 h-6 rounded-md bg-slate-200/80">
          <Server className="h-3.5 w-3.5 text-slate-600" />
        </div>
        <span className="font-semibold text-sm text-slate-700 truncate">{data.hostname}</span>
        <div className="ml-auto flex items-center gap-1.5">
          <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
            {data.serviceCount} {data.serviceCount === 1 ? 'service' : 'services'}
          </Badge>
          <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" title="Agent connected" />
        </div>
      </div>
    </div>
  );
}

export const HostGroupNode = memo(HostGroupNodeInner);
