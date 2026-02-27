import { memo } from 'react';
import { Handle, Position } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import { Cloud } from 'lucide-react';
import type { ExternalNodeData } from './TopologyMap.types';

function ExternalNodeInner({ data }: NodeProps & { data: ExternalNodeData }) {
  return (
    <div className="rounded-lg border-2 border-dashed border-slate-300 bg-slate-50/50 w-[160px] backdrop-blur-sm">
      <Handle type="target" position={Position.Top} className="!bg-slate-300 !w-2 !h-2" />

      <div className="p-2.5 flex items-center gap-2">
        <Cloud className="h-4 w-4 text-slate-400 flex-shrink-0" />
        <div className="min-w-0">
          <div className="text-xs font-medium text-slate-500 truncate">
            {data.address}
          </div>
          <div className="text-[10px] font-mono text-slate-400">
            :{data.port}
          </div>
        </div>
      </div>

      <Handle type="source" position={Position.Bottom} className="!bg-slate-300 !w-2 !h-2" />
    </div>
  );
}

export const ExternalNode = memo(ExternalNodeInner);
