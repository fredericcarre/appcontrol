import { X, Play, Square, RotateCcw, Terminal, Search, Server, Clock, Shield, Skull, GitBranch, ArrowRight, Wrench } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { STATE_COLORS, ComponentState } from '@/lib/colors';
import { Component } from '@/api/apps';
import { useStateTransitions, useCommandExecutions } from '@/api/components';

interface DetailPanelProps {
  component: Component;
  onClose: () => void;
  onStart?: () => void;
  onStop?: () => void;
  onRestart?: () => void;
  onCommand?: () => void;
  onDiagnose?: () => void;
  onForceStop?: () => void;
  onStartWithDeps?: () => void;
  onRepair?: () => void;
  canOperate?: boolean;
}

function stateColor(state: string): string {
  switch (state.toUpperCase()) {
    case 'RUNNING': return 'text-emerald-500';
    case 'FAILED': return 'text-red-500';
    case 'DEGRADED': return 'text-amber-500';
    case 'STOPPED': return 'text-gray-400';
    case 'STARTING': return 'text-blue-500';
    case 'STOPPING': return 'text-orange-400';
    default: return 'text-gray-500';
  }
}

function stateDot(state: string): string {
  switch (state.toUpperCase()) {
    case 'RUNNING': return 'bg-emerald-500';
    case 'FAILED': return 'bg-red-500';
    case 'DEGRADED': return 'bg-amber-500';
    case 'STOPPED': return 'bg-gray-400';
    case 'STARTING': case 'STOPPING': return 'bg-blue-500';
    default: return 'bg-gray-400';
  }
}

function timeAgo(dateStr: string): string {
  const diffS = Math.floor((Date.now() - new Date(dateStr).getTime()) / 1000);
  if (diffS < 60) return `${diffS}s ago`;
  const diffM = Math.floor(diffS / 60);
  if (diffM < 60) return `${diffM}m ago`;
  const diffH = Math.floor(diffM / 60);
  if (diffH < 24) return `${diffH}h ago`;
  return `${Math.floor(diffH / 24)}d ago`;
}

export function DetailPanel({
  component,
  onClose,
  onStart,
  onStop,
  onRestart,
  onCommand,
  onDiagnose,
  onForceStop,
  onStartWithDeps,
  onRepair,
  canOperate,
}: DetailPanelProps) {
  const state = (component.current_state || 'UNKNOWN') as ComponentState;
  const stateStyle = STATE_COLORS[state] || STATE_COLORS.UNKNOWN;
  const { data: transitions } = useStateTransitions(component.id);
  const { data: executions } = useCommandExecutions(component.id, 10);

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
          <div className="space-y-2">
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
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={onStartWithDeps} className="flex-1">
                <GitBranch className="h-3.5 w-3.5 mr-1" /> Start with deps
              </Button>
              <Button variant="destructive" size="sm" onClick={onForceStop} className="flex-1">
                <Skull className="h-3.5 w-3.5 mr-1" /> Force Kill
              </Button>
            </div>
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={onRepair} className="flex-1">
                <Wrench className="h-3.5 w-3.5 mr-1" /> Repair
              </Button>
            </div>
          </div>
        )}
      </div>

      <Separator />

      <Tabs defaultValue="info" className="flex-1 flex flex-col min-h-0">
        <TabsList className="mx-4 mt-2">
          <TabsTrigger value="info">Info</TabsTrigger>
          <TabsTrigger value="commands">Commands</TabsTrigger>
          <TabsTrigger value="events">Events</TabsTrigger>
        </TabsList>

        <TabsContent value="info" className="flex-1 overflow-auto p-4 space-y-3">
          <InfoRow icon={Server} label="Host" value={component.host || 'N/A'} />
          <InfoRow icon={Clock} label="Check Interval" value={`${component.check_interval_seconds || 30}s`} />
          <InfoRow icon={Shield} label="Optional" value={component.is_optional ? 'Yes' : 'No'} />
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

            {/* Recent command executions */}
            {executions && executions.length > 0 && (
              <div className="mt-4 space-y-2">
                <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                  Recent Executions
                </h4>
                {executions.map((exec) => (
                  <div key={exec.id} className="border rounded-md p-2 space-y-1">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-mono">{exec.command_type}</span>
                      <Badge
                        variant={exec.status === 'completed' ? 'running' : exec.status === 'failed' ? 'failed' : 'outline'}
                        className="text-[10px] h-4"
                      >
                        {exec.status}
                      </Badge>
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      {timeAgo(exec.dispatched_at)}
                      {exec.duration_ms != null && ` · ${exec.duration_ms}ms`}
                      {exec.exit_code != null && ` · exit ${exec.exit_code}`}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </TabsContent>

        <TabsContent value="events" className="flex-1 min-h-0">
          <ScrollArea className="h-full">
            <div className="p-4 space-y-0">
              {!transitions || transitions.length === 0 ? (
                <p className="text-sm text-muted-foreground text-center py-8">
                  No state changes recorded
                </p>
              ) : (
                transitions.map((t, i) => (
                  <div key={t.id} className="flex gap-2.5 pb-3">
                    {/* Timeline dot + line */}
                    <div className="flex flex-col items-center pt-1">
                      <div className={`h-2 w-2 rounded-full ${stateDot(t.to_state)}`} />
                      {i < transitions.length - 1 && (
                        <div className="w-px flex-1 bg-border mt-1" />
                      )}
                    </div>

                    {/* Content */}
                    <div className="flex-1 min-w-0 space-y-0.5 pb-1">
                      <div className="flex items-center gap-1 text-xs">
                        <span className={`opacity-60 ${stateColor(t.from_state)}`}>
                          {t.from_state}
                        </span>
                        <ArrowRight className="h-2.5 w-2.5 text-muted-foreground flex-shrink-0" />
                        <span className={`font-semibold ${stateColor(t.to_state)}`}>
                          {t.to_state}
                        </span>
                      </div>
                      <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
                        <span>{new Date(t.created_at).toLocaleString()}</span>
                        {t.trigger !== 'check' && (
                          <>
                            <span>·</span>
                            <span className="font-medium">{t.trigger}</span>
                          </>
                        )}
                      </div>
                    </div>

                    {/* Time ago */}
                    <span className="text-[10px] text-muted-foreground whitespace-nowrap pt-1">
                      {timeAgo(t.created_at)}
                    </span>
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
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
