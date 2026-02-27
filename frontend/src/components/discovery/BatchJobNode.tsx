import { memo } from 'react';
import type { NodeProps } from '@xyflow/react';
import { Clock } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import type { BatchJobNodeData } from './TopologyMap.types';

function BatchJobNodeInner({ data }: NodeProps & { data: BatchJobNodeData }) {
  return (
    <div className="rounded-lg border-2 border-amber-300 bg-amber-50/80 w-[180px] shadow-sm">
      <div className="p-2.5">
        <div className="flex items-center gap-1.5 mb-1">
          <Clock className="h-3.5 w-3.5 text-amber-600 flex-shrink-0" />
          <span className="font-semibold text-xs text-amber-900 truncate">{data.name}</span>
        </div>
        <div className="text-[10px] font-mono text-amber-700 truncate mb-1" title={data.schedule}>
          {data.schedule}
        </div>
        <div className="flex items-center gap-1">
          <Badge variant="outline" className="text-[9px] px-1 py-0 border-amber-300 text-amber-700">
            {data.source}
          </Badge>
          <span className="text-[9px] text-muted-foreground truncate">{data.hostname}</span>
        </div>
      </div>
    </div>
  );
}

export const BatchJobNode = memo(BatchJobNodeInner);
