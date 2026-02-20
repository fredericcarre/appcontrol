import { X, Play, Square, RotateCcw, Terminal, Search, Server, Clock, Shield } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { STATE_COLORS, ComponentState } from '@/lib/colors';
import { Component } from '@/api/apps';

interface DetailPanelProps {
  component: Component;
  onClose: () => void;
  onStart?: () => void;
  onStop?: () => void;
  onRestart?: () => void;
  onCommand?: () => void;
  onDiagnose?: () => void;
  canOperate?: boolean;
}

export function DetailPanel({
  component,
  onClose,
  onStart,
  onStop,
  onRestart,
  onCommand,
  onDiagnose,
  canOperate,
}: DetailPanelProps) {
  const state = (component.state || 'UNKNOWN') as ComponentState;
  const stateStyle = STATE_COLORS[state] || STATE_COLORS.UNKNOWN;

  return (
    <div className="w-[360px] border-l border-border bg-card h-full flex flex-col">
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div>
          <h3 className="font-semibold text-sm">{component.name}</h3>
          <p className="text-xs text-muted-foreground">{component.host}</p>
        </div>
        <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onClose}>
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div className="p-4 space-y-3">
        <div className="flex items-center gap-2">
          <div
            className="w-3 h-3 rounded-full"
            style={{ backgroundColor: stateStyle.border }}
          />
          <span className="text-sm font-medium">{state}</span>
          <Badge variant="outline" className="ml-auto text-xs">
            {component.component_type}
          </Badge>
        </div>

        {canOperate && (
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={onStart} className="flex-1">
              <Play className="h-3.5 w-3.5 mr-1" /> Start
            </Button>
            <Button variant="outline" size="sm" onClick={onStop} className="flex-1">
              <Square className="h-3.5 w-3.5 mr-1" /> Stop
            </Button>
            <Button variant="outline" size="sm" onClick={onRestart}>
              <RotateCcw className="h-3.5 w-3.5" />
            </Button>
          </div>
        )}
      </div>

      <Separator />

      <Tabs defaultValue="info" className="flex-1 flex flex-col">
        <TabsList className="mx-4 mt-2">
          <TabsTrigger value="info">Info</TabsTrigger>
          <TabsTrigger value="commands">Commands</TabsTrigger>
          <TabsTrigger value="events">Events</TabsTrigger>
        </TabsList>

        <TabsContent value="info" className="flex-1 overflow-auto p-4 space-y-3">
          <InfoRow icon={Server} label="Host" value={component.host} />
          <InfoRow icon={Clock} label="Check Interval" value={`${component.check_interval_secs}s`} />
          <InfoRow icon={Shield} label="Protected" value={component.is_protected ? 'Yes' : 'No'} />
          {component.check_cmd && <InfoRow icon={Terminal} label="Check CMD" value={component.check_cmd} />}
          {component.start_cmd && <InfoRow icon={Play} label="Start CMD" value={component.start_cmd} />}
          {component.stop_cmd && <InfoRow icon={Square} label="Stop CMD" value={component.stop_cmd} />}
        </TabsContent>

        <TabsContent value="commands" className="flex-1 overflow-auto p-4">
          <div className="space-y-2">
            {canOperate && (
              <>
                <Button variant="outline" className="w-full justify-start" onClick={onCommand}>
                  <Terminal className="h-4 w-4 mr-2" /> Execute Custom Command
                </Button>
                <Button variant="outline" className="w-full justify-start" onClick={onDiagnose}>
                  <Search className="h-4 w-4 mr-2" /> Run Diagnostic
                </Button>
              </>
            )}
          </div>
        </TabsContent>

        <TabsContent value="events" className="flex-1 overflow-auto p-4">
          <p className="text-sm text-muted-foreground">Recent events will appear here via WebSocket.</p>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function InfoRow({ icon: Icon, label, value }: { icon: React.ComponentType<{ className?: string }>; label: string; value: string }) {
  return (
    <div className="flex items-start gap-2 text-sm">
      <Icon className="h-4 w-4 text-muted-foreground mt-0.5 shrink-0" />
      <div>
        <span className="text-muted-foreground">{label}:</span>{' '}
        <span className="font-medium break-all">{value}</span>
      </div>
    </div>
  );
}
