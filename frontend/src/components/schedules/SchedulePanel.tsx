import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Calendar, X } from 'lucide-react';
import { ScheduleList } from './ScheduleList';

interface SchedulePanelProps {
  appId: string;
  canOperate?: boolean;
  onClose: () => void;
}

export function SchedulePanel({ appId, canOperate = false, onClose }: SchedulePanelProps) {
  return (
    <div className="w-[380px] border-l border-border bg-card h-full flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div className="flex items-center gap-2">
          <Calendar className="h-4 w-4 text-primary" />
          <h2 className="font-semibold text-sm">Schedules</h2>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose} className="h-7 w-7 p-0">
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Content */}
      <ScrollArea className="flex-1">
        <div className="p-4">
          <ScheduleList appId={appId} canOperate={canOperate} />
        </div>
      </ScrollArea>
    </div>
  );
}
