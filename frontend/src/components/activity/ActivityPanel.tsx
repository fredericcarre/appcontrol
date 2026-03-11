import { useMemo } from 'react';
import {
  useActivityFeed,
  useHealthSummary,
  type ActivityEvent,
} from '@/api/apps';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { Button } from '@/components/ui/button';
import {
  Activity,
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  ChevronRight,
  Clock,
  Cpu,
  Play,
  Power,
  PowerOff,
  Radio,
  Server,
  Square,
  Terminal,
  User,
  X,
  XCircle,
  Zap,
} from 'lucide-react';

interface ActivityPanelProps {
  appId: string;
  onClose: () => void;
  onSelectComponent?: (componentId: string) => void;
}

// ── State color utilities ──────────────────────────────────────

function stateColor(state: string): string {
  switch (state.toUpperCase()) {
    case 'RUNNING':
      return 'text-emerald-500';
    case 'FAILED':
      return 'text-red-500';
    case 'DEGRADED':
      return 'text-amber-500';
    case 'STOPPED':
      return 'text-gray-400';
    case 'STARTING':
      return 'text-blue-500';
    case 'STOPPING':
      return 'text-orange-400';
    case 'UNREACHABLE':
      return 'text-red-400';
    default:
      return 'text-gray-500';
  }
}

function stateDot(state: string): string {
  switch (state.toUpperCase()) {
    case 'RUNNING':
      return 'bg-emerald-500';
    case 'FAILED':
      return 'bg-red-500';
    case 'DEGRADED':
      return 'bg-amber-500';
    case 'STOPPED':
      return 'bg-gray-400';
    case 'STARTING':
    case 'STOPPING':
      return 'bg-blue-500 animate-pulse';
    case 'UNREACHABLE':
      return 'bg-red-400 animate-pulse';
    default:
      return 'bg-gray-400';
  }
}

// ── Time formatting ────────────────────────────────────────────

function timeAgo(dateStr: string): string {
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const diffS = Math.floor((now - then) / 1000);
  if (diffS < 60) return `${diffS}s ago`;
  const diffM = Math.floor(diffS / 60);
  if (diffM < 60) return `${diffM}m ago`;
  const diffH = Math.floor(diffM / 60);
  if (diffH < 24) return `${diffH}h ago`;
  const diffD = Math.floor(diffH / 24);
  return `${diffD}d ago`;
}

// ── Health Summary Cards ───────────────────────────────────────

function HealthCards({ appId }: { appId: string }) {
  const { data: health } = useHealthSummary(appId);
  if (!health) return null;

  const running =
    health.state_breakdown.find((s) => s.state === 'RUNNING')?.count ?? 0;
  const failed =
    health.state_breakdown.find((s) => s.state === 'FAILED')?.count ?? 0;
  const degraded =
    health.state_breakdown.find((s) => s.state === 'DEGRADED')?.count ?? 0;
  const unreachable =
    health.state_breakdown.find((s) => s.state === 'UNREACHABLE')?.count ?? 0;
  const stopped =
    health.state_breakdown.find((s) => s.state === 'STOPPED')?.count ?? 0;

  const agentsOk = health.agents.filter((a) => a.active && !a.stale).length;
  const agentsDown = health.agents.filter((a) => !a.active || a.stale).length;

  return (
    <div className="px-4 pb-3 space-y-3">
      {/* State bar */}
      <div className="flex h-2 rounded-full overflow-hidden bg-muted">
        {running > 0 && (
          <div
            className="bg-emerald-500 transition-all"
            style={{ width: `${(running / health.total_components) * 100}%` }}
          />
        )}
        {degraded > 0 && (
          <div
            className="bg-amber-500 transition-all"
            style={{
              width: `${(degraded / health.total_components) * 100}%`,
            }}
          />
        )}
        {failed > 0 && (
          <div
            className="bg-red-500 transition-all"
            style={{ width: `${(failed / health.total_components) * 100}%` }}
          />
        )}
        {unreachable > 0 && (
          <div
            className="bg-red-400 transition-all"
            style={{
              width: `${(unreachable / health.total_components) * 100}%`,
            }}
          />
        )}
        {stopped > 0 && (
          <div
            className="bg-gray-400 transition-all"
            style={{
              width: `${(stopped / health.total_components) * 100}%`,
            }}
          />
        )}
      </div>

      {/* Quick stats row */}
      <div className="grid grid-cols-4 gap-2">
        <MiniStat icon={CheckCircle2} value={running} label="Running" color="text-emerald-500" />
        <MiniStat icon={XCircle} value={failed} label="Failed" color="text-red-500" />
        <MiniStat icon={AlertTriangle} value={degraded + unreachable} label="Degraded" color="text-amber-500" />
        <MiniStat icon={Square} value={stopped} label="Stopped" color="text-gray-400" />
      </div>

      {/* Agents row */}
      <div className="flex items-center justify-between text-xs">
        <div className="flex items-center gap-1.5">
          <Server className="h-3.5 w-3.5 text-muted-foreground" />
          <span className="text-muted-foreground">Agents</span>
        </div>
        <div className="flex items-center gap-2">
          {agentsOk > 0 && (
            <span className="flex items-center gap-1 text-emerald-500">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
              {agentsOk} connected
            </span>
          )}
          {agentsDown > 0 && (
            <span className="flex items-center gap-1 text-red-500">
              <span className="h-1.5 w-1.5 rounded-full bg-red-500 animate-pulse" />
              {agentsDown} down
            </span>
          )}
        </div>
      </div>

      {/* Error components */}
      {health.error_components.length > 0 && (
        <div className="border rounded-lg border-red-500/20 bg-red-500/5 p-2.5 space-y-1.5">
          <div className="flex items-center gap-1.5 text-xs font-medium text-red-500">
            <Zap className="h-3.5 w-3.5" />
            Components in error
          </div>
          {health.error_components.map((ec) => (
            <div
              key={ec.component_id}
              className="flex items-center justify-between text-xs"
            >
              <span className="font-medium truncate flex-1">{ec.name}</span>
              <Badge
                variant={
                  ec.state === 'FAILED'
                    ? 'failed'
                    : ec.state === 'DEGRADED'
                      ? 'degraded'
                      : 'destructive'
                }
                className="text-[10px] h-5"
              >
                {ec.state}
              </Badge>
            </div>
          ))}
        </div>
      )}

      {/* Recent incidents */}
      {health.recent_incidents.length > 0 && (
        <div className="border rounded-lg border-amber-500/20 bg-amber-500/5 p-2.5 space-y-1.5">
          <div className="flex items-center gap-1.5 text-xs font-medium text-amber-600 dark:text-amber-400">
            <AlertTriangle className="h-3.5 w-3.5" />
            Recent incidents
          </div>
          {health.recent_incidents.slice(0, 5).map((inc, i) => (
            <div key={i} className="flex items-center justify-between text-xs">
              <span className="truncate flex-1">{inc.component_name}</span>
              <span className="text-muted-foreground ml-2 whitespace-nowrap">
                {timeAgo(inc.at)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function MiniStat({
  icon: Icon,
  value,
  label,
  color,
}: {
  icon: React.ComponentType<{ className?: string }>;
  value: number;
  label: string;
  color: string;
}) {
  return (
    <div className="flex flex-col items-center gap-0.5 py-1.5">
      <div className={`text-lg font-semibold leading-none ${color}`}>
        {value}
      </div>
      <div className="flex items-center gap-0.5 text-[10px] text-muted-foreground">
        <Icon className={`h-3 w-3 ${color}`} />
        {label}
      </div>
    </div>
  );
}

// ── Timeline Event Renderer ────────────────────────────────────

function TimelineEvent({
  event,
  onSelectComponent,
}: {
  event: ActivityEvent;
  onSelectComponent?: (id: string) => void;
}) {
  const handleClick = () => {
    if (event.component_id && onSelectComponent) {
      onSelectComponent(event.component_id);
    }
  };

  if (event.kind === 'state_change') {
    return (
      <div
        className="group flex gap-3 py-2 px-3 hover:bg-muted/50 rounded-md cursor-pointer transition-colors"
        onClick={handleClick}
      >
        <div className="flex flex-col items-center pt-0.5">
          <div className={`h-2.5 w-2.5 rounded-full ${stateDot(event.to_state ?? '')}`} />
          <div className="w-px flex-1 bg-border mt-1" />
        </div>
        <div className="flex-1 min-w-0 space-y-0.5">
          <div className="flex items-center gap-1.5 text-sm">
            <span className="font-medium truncate">
              {event.component_name}
            </span>
            <ChevronRight className="h-3 w-3 text-muted-foreground flex-shrink-0" />
            <span
              className={`font-semibold ${stateColor(event.to_state ?? '')}`}
            >
              {event.to_state}
            </span>
          </div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span className={`${stateColor(event.from_state ?? '')} opacity-60`}>
              {event.from_state}
            </span>
            <ArrowRight className="h-2.5 w-2.5" />
            <span className={stateColor(event.to_state ?? '')}>
              {event.to_state}
            </span>
            {event.trigger && event.trigger !== 'check' && (
              <>
                <span>·</span>
                <span>{event.trigger}</span>
              </>
            )}
          </div>
        </div>
        <span className="text-[10px] text-muted-foreground whitespace-nowrap pt-0.5">
          {timeAgo(event.at)}
        </span>
      </div>
    );
  }

  if (event.kind === 'user_action') {
    const actionIcon = getActionIcon(event.action ?? '');
    return (
      <div
        className="group flex gap-3 py-2 px-3 hover:bg-muted/50 rounded-md cursor-pointer transition-colors"
        onClick={handleClick}
      >
        <div className="flex flex-col items-center pt-0.5">
          <div className="h-5 w-5 rounded-full bg-blue-500/10 flex items-center justify-center">
            {actionIcon}
          </div>
          <div className="w-px flex-1 bg-border mt-1" />
        </div>
        <div className="flex-1 min-w-0 space-y-0.5">
          <div className="text-sm">
            <span className="font-medium text-blue-500 dark:text-blue-400">
              {event.user?.split('@')[0] ?? 'system'}
            </span>
            <span className="text-muted-foreground"> {formatAction(event.action ?? '')}</span>
            {event.component_name && (
              <span className="font-medium"> {event.component_name}</span>
            )}
          </div>
        </div>
        <span className="text-[10px] text-muted-foreground whitespace-nowrap pt-0.5">
          {timeAgo(event.at)}
        </span>
      </div>
    );
  }

  if (event.kind === 'command') {
    const success = event.exit_code === 0;
    const pending = event.exit_code == null;
    return (
      <div
        className="group flex gap-3 py-2 px-3 hover:bg-muted/50 rounded-md cursor-pointer transition-colors"
        onClick={handleClick}
      >
        <div className="flex flex-col items-center pt-0.5">
          <div
            className={`h-5 w-5 rounded-full flex items-center justify-center ${
              pending
                ? 'bg-yellow-500/10'
                : success
                  ? 'bg-emerald-500/10'
                  : 'bg-red-500/10'
            }`}
          >
            <Terminal
              className={`h-3 w-3 ${
                pending
                  ? 'text-yellow-500'
                  : success
                    ? 'text-emerald-500'
                    : 'text-red-500'
              }`}
            />
          </div>
          <div className="w-px flex-1 bg-border mt-1" />
        </div>
        <div className="flex-1 min-w-0 space-y-0.5">
          <div className="flex items-center gap-1.5 text-sm">
            <span className="font-mono text-xs px-1.5 py-0.5 rounded bg-muted">
              {event.command_type}
            </span>
            <span className="text-muted-foreground">on</span>
            <span className="font-medium truncate">
              {event.component_name}
            </span>
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            {event.exit_code != null && (
              <span className={success ? 'text-emerald-500' : 'text-red-500'}>
                exit {event.exit_code}
              </span>
            )}
            {event.duration_ms != null && (
              <span>{event.duration_ms}ms</span>
            )}
            {pending && (
              <span className="text-yellow-500 flex items-center gap-1">
                <Radio className="h-3 w-3 animate-pulse" /> running
              </span>
            )}
          </div>
        </div>
        <span className="text-[10px] text-muted-foreground whitespace-nowrap pt-0.5">
          {timeAgo(event.at)}
        </span>
      </div>
    );
  }

  return null;
}

function getActionIcon(action: string) {
  if (action.includes('start'))
    return <Play className="h-3 w-3 text-emerald-500" />;
  if (action.includes('stop'))
    return <PowerOff className="h-3 w-3 text-red-500" />;
  if (action.includes('restart'))
    return <Power className="h-3 w-3 text-amber-500" />;
  if (action.includes('command') || action.includes('execute'))
    return <Terminal className="h-3 w-3 text-blue-500" />;
  if (action.includes('diagnose'))
    return <Cpu className="h-3 w-3 text-purple-500" />;
  return <User className="h-3 w-3 text-blue-500" />;
}

function formatAction(action: string): string {
  return action
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase())
    .replace('App', 'application')
    .toLowerCase();
}

// ── Main Panel ─────────────────────────────────────────────────

export function ActivityPanel({
  appId,
  onClose,
  onSelectComponent,
}: ActivityPanelProps) {
  const { data: events, isLoading } = useActivityFeed(appId);

  // Deduplicate events by unique key (kind + component_id + at)
  const deduplicatedEvents = useMemo(() => {
    if (!events) return [];
    const seen = new Set<string>();
    return events.filter((event) => {
      const key = `${event.kind}-${event.component_id || 'app'}-${event.at}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, [events]);

  // Group events by day
  const groupedEvents = useMemo(() => {
    if (!deduplicatedEvents || deduplicatedEvents.length === 0) return [];
    const groups: Array<{ label: string; events: ActivityEvent[] }> = [];
    let currentLabel = '';

    for (const event of deduplicatedEvents) {
      const date = new Date(event.at);
      const today = new Date();
      const yesterday = new Date();
      yesterday.setDate(yesterday.getDate() - 1);

      let label: string;
      if (date.toDateString() === today.toDateString()) {
        label = 'Today';
      } else if (date.toDateString() === yesterday.toDateString()) {
        label = 'Yesterday';
      } else {
        label = date.toLocaleDateString(undefined, {
          weekday: 'short',
          month: 'short',
          day: 'numeric',
        });
      }

      if (label !== currentLabel) {
        groups.push({ label, events: [] });
        currentLabel = label;
      }
      groups[groups.length - 1].events.push(event);
    }

    return groups;
  }, [deduplicatedEvents]);

  return (
    <div className="w-[380px] border-l border-border bg-card h-full flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div className="flex items-center gap-2">
          <Activity className="h-4 w-4 text-primary" />
          <h2 className="font-semibold text-sm">Activity</h2>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose} className="h-7 w-7 p-0">
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Health summary */}
      <div className="pt-3">
        <HealthCards appId={appId} />
      </div>

      <Separator />

      {/* Timeline */}
      <ScrollArea className="flex-1">
        <div className="py-2">
          {isLoading && (
            <div className="flex items-center justify-center py-12">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Clock className="h-4 w-4 animate-spin" />
                Loading activity...
              </div>
            </div>
          )}

          {!isLoading && (!events || events.length === 0) && (
            <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
              <Activity className="h-8 w-8 mb-2 opacity-30" />
              <p className="text-sm">No activity yet</p>
            </div>
          )}

          {groupedEvents.map((group) => (
            <div key={group.label}>
              <div className="sticky top-0 z-10 bg-card/95 backdrop-blur-sm px-4 py-1.5">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                  {group.label}
                </span>
              </div>
              {group.events.map((event, i) => (
                <TimelineEvent
                  key={`${event.kind}-${event.at}-${i}`}
                  event={event}
                  onSelectComponent={onSelectComponent}
                />
              ))}
            </div>
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}
