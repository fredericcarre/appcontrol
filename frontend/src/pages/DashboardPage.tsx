import { useMemo, useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useApps, useStartApp, useStopApp, useCancelOperation } from '@/api/apps';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { ConfirmDialog } from '@/components/ui/confirm-dialog';
import { useWebSocketStore } from '@/stores/websocket';
import {
  Sun, CloudSun, Cloud, CloudRain, CloudLightning,
  Plus, Activity, AlertTriangle, CheckCircle, XCircle,
  Play, Square, Loader2, ArrowRight, Terminal, Wifi, WifiOff,
  RefreshCw, Command, Ban,
} from 'lucide-react';
import { WsMessage } from '@/stores/websocket';

const weatherIcons: Record<string, React.ComponentType<{ className?: string }>> = {
  sunny: Sun,
  fair: CloudSun,
  cloudy: Cloud,
  rainy: CloudRain,
  stormy: CloudLightning,
};

function WeatherIcon({ weather, className }: { weather: string; className?: string }) {
  const Icon = weatherIcons[weather] || Cloud;
  return <Icon className={className} />;
}

function getWeatherVariant(weather: string) {
  if (weather === 'sunny') return 'running' as const;
  if (weather === 'stormy') return 'failed' as const;
  if (weather === 'rainy') return 'degraded' as const;
  return 'secondary' as const;
}

function getStateColor(state: string) {
  switch (state) {
    case 'RUNNING': return 'text-green-600';
    case 'STOPPED': return 'text-gray-500';
    case 'FAILED': return 'text-red-600';
    case 'DEGRADED': return 'text-amber-600';
    case 'STARTING': return 'text-blue-500';
    case 'STOPPING': return 'text-blue-500';
    default: return 'text-gray-400';
  }
}

/** Format a WebSocket event for display in Live Events */
function formatEvent(ev: WsMessage): { icon: React.ReactNode; text: string; color: string; context?: string } {
  const payload = ev.payload || {};

  // Extract component/app names for context
  const compName = payload.component_name as string | undefined;
  const appName = payload.app_name as string | undefined;
  const context = compName ? `${appName ? appName + ' / ' : ''}${compName}` : undefined;

  switch (ev.type) {
    case 'StateChange': {
      const from = String(payload.from || '?');
      const to = String(payload.to || '?');
      return {
        icon: <ArrowRight className="h-3 w-3" />,
        text: `${from} → ${to}`,
        color: getStateColor(to),
        context,
      };
    }

    case 'CheckResultEvent': {
      const exitCode = payload.exit_code as number;
      const checkType = String(payload.check_type || 'health').toLowerCase();
      const isOk = exitCode === 0;
      return {
        icon: isOk ? <CheckCircle className="h-3 w-3" /> : <XCircle className="h-3 w-3" />,
        text: `${checkType}: ${isOk ? 'OK' : `exit ${exitCode}`}`,
        color: isOk ? 'text-green-600' : (exitCode === 1 ? 'text-amber-500' : 'text-red-600'),
        context,
      };
    }

    case 'CommandResultEvent': {
      const exitCode = payload.exit_code as number;
      const isOk = exitCode === 0;
      return {
        icon: <Command className="h-3 w-3" />,
        text: `cmd: ${isOk ? 'OK' : `exit ${exitCode}`}`,
        color: isOk ? 'text-green-600' : 'text-red-600',
        context: compName,
      };
    }

    case 'AgentStatus': {
      const connected = Boolean(payload.connected);
      return {
        icon: connected ? <Wifi className="h-3 w-3" /> : <WifiOff className="h-3 w-3" />,
        text: connected ? 'agent connected' : 'agent disconnected',
        color: connected ? 'text-green-600' : 'text-amber-500',
      };
    }

    case 'SwitchoverProgress': {
      const phase = String(payload.phase || '?');
      const status = String(payload.status || '?');
      return {
        icon: <RefreshCw className="h-3 w-3" />,
        text: `switchover: ${phase} (${status})`,
        color: status === 'completed' ? 'text-green-600' : 'text-blue-500',
      };
    }

    case 'TerminalStarted':
    case 'TerminalOutput':
    case 'TerminalExit':
    case 'TerminalError': {
      return {
        icon: <Terminal className="h-3 w-3" />,
        text: ev.type.replace('Terminal', 'terminal ').toLowerCase(),
        color: 'text-gray-500',
      };
    }

    case 'LogEntry': {
      const level = String(payload.level || 'INFO');
      const source = String(payload.source_name || 'unknown');
      const message = String(payload.message || '').slice(0, 50);
      const levelColor = level === 'ERROR' ? 'text-red-600' :
                         level === 'WARN' ? 'text-amber-500' : 'text-gray-500';
      return {
        icon: null,
        text: `[${source}] ${message}${message.length >= 50 ? '...' : ''}`,
        color: levelColor,
      };
    }

    case 'AutoFailover': {
      const fromProfile = String(payload.from_profile || '?');
      const toProfile = String(payload.to_profile || '?');
      return {
        icon: <AlertTriangle className="h-3 w-3" />,
        text: `auto-failover: ${fromProfile} → ${toProfile}`,
        color: 'text-red-600',
      };
    }

    default:
      return {
        icon: null,
        text: ev.type,
        color: 'text-gray-500',
      };
  }
}

export function DashboardPage() {
  const { data: apps, isLoading, refetch } = useApps();
  const startApp = useStartApp();
  const stopApp = useStopApp();
  const cancelOperation = useCancelOperation();
  const messages = useWebSocketStore((s) => s.messages);
  const navigate = useNavigate();

  // Track which app is being operated on
  const [operatingAppId, setOperatingAppId] = useState<string | null>(null);
  const [operationType, setOperationType] = useState<'start' | 'stop' | null>(null);

  // Confirm dialog state
  const [confirmDialog, setConfirmDialog] = useState<{
    open: boolean;
    title: string;
    description: string;
    confirmLabel: string;
    variant: 'default' | 'destructive' | 'warning';
    onConfirm: () => void;
  }>({
    open: false,
    title: '',
    description: '',
    confirmLabel: 'Confirm',
    variant: 'default',
    onConfirm: () => {},
  });

  const stats = useMemo(() => {
    if (!apps) return { total: 0, running: 0, transitioning: 0, degraded: 0, failed: 0 };
    return {
      total: apps.length,
      running: apps.filter((a) => a.global_state === 'RUNNING').length,
      transitioning: apps.filter((a) => a.global_state === 'STARTING' || a.global_state === 'STOPPING').length,
      degraded: apps.filter((a) => a.global_state === 'DEGRADED' || a.global_state === 'STOPPED').length,
      failed: apps.filter((a) => a.global_state === 'FAILED').length,
    };
  }, [apps]);

  const handleStart = useCallback((e: React.MouseEvent, appId: string, appName: string) => {
    e.stopPropagation();
    setConfirmDialog({
      open: true,
      title: 'Start Application',
      description: `Start all components of "${appName}"?`,
      confirmLabel: 'Start',
      variant: 'default',
      onConfirm: () => {
        setOperatingAppId(appId);
        setOperationType('start');
        startApp.mutate(appId, {
          onSettled: () => {
            setOperatingAppId(null);
            setOperationType(null);
            refetch();
            setTimeout(() => refetch(), 1000);
          },
        });
      },
    });
  }, [startApp, refetch]);

  const handleStop = useCallback((e: React.MouseEvent, appId: string, appName: string) => {
    e.stopPropagation();
    setConfirmDialog({
      open: true,
      title: 'Stop Application',
      description: `Stop all components of "${appName}"?`,
      confirmLabel: 'Stop',
      variant: 'destructive',
      onConfirm: () => {
        setOperatingAppId(appId);
        setOperationType('stop');
        stopApp.mutate(appId, {
          onSettled: () => {
            setOperatingAppId(null);
            setOperationType(null);
            refetch();
            setTimeout(() => refetch(), 1000);
          },
        });
      },
    });
  }, [stopApp, refetch]);

  const handleCancel = useCallback((e: React.MouseEvent, appId: string) => {
    e.stopPropagation();
    setConfirmDialog({
      open: true,
      title: 'Cancel Operation',
      description: 'Cancel the current operation and release the lock?',
      confirmLabel: 'Cancel Operation',
      variant: 'warning',
      onConfirm: () => {
        cancelOperation.mutate(appId, {
          onSuccess: () => {
            setOperatingAppId(null);
            setOperationType(null);
            setTimeout(() => refetch(), 500);
          },
        });
      },
    });
  }, [cancelOperation, refetch]);

  // Filter out terminal events (not useful in dashboard) and LogEntry (too verbose)
  const recentEvents = messages
    .filter((ev) => !ev.type.startsWith('Terminal') && ev.type !== 'LogEntry')
    .slice(-20)
    .reverse();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Dashboard</h1>
        <Button onClick={() => navigate('/onboarding')}>
          <Plus className="h-4 w-4 mr-2" /> New Application
        </Button>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
        <Card>
          <CardContent className="p-4 flex items-center gap-3">
            <Activity className="h-8 w-8 text-primary" />
            <div>
              <p className="text-2xl font-bold">{stats.total}</p>
              <p className="text-xs text-muted-foreground">Total Apps</p>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 flex items-center gap-3">
            <CheckCircle className="h-8 w-8 text-green-600" />
            <div>
              <p className="text-2xl font-bold">{stats.running}</p>
              <p className="text-xs text-muted-foreground">Running</p>
            </div>
          </CardContent>
        </Card>
        {stats.transitioning > 0 && (
          <Card>
            <CardContent className="p-4 flex items-center gap-3">
              <Loader2 className="h-8 w-8 text-blue-500 animate-spin" />
              <div>
                <p className="text-2xl font-bold">{stats.transitioning}</p>
                <p className="text-xs text-muted-foreground">In Transition</p>
              </div>
            </CardContent>
          </Card>
        )}
        <Card>
          <CardContent className="p-4 flex items-center gap-3">
            <AlertTriangle className="h-8 w-8 text-amber-500" />
            <div>
              <p className="text-2xl font-bold">{stats.degraded}</p>
              <p className="text-xs text-muted-foreground">Degraded / Stopped</p>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 flex items-center gap-3">
            <XCircle className="h-8 w-8 text-red-600" />
            <div>
              <p className="text-2xl font-bold">{stats.failed}</p>
              <p className="text-xs text-muted-foreground">Failed</p>
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2">
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Applications</CardTitle>
            </CardHeader>
            <CardContent>
              {!apps?.length ? (
                <p className="text-sm text-muted-foreground py-8 text-center">
                  No applications yet. Create one to get started.
                </p>
              ) : (
                <div className="space-y-2">
                  {apps.map((app) => (
                    <div
                      key={app.id}
                      onClick={() => navigate(`/apps/${app.id}`)}
                      className="w-full flex items-center gap-3 p-3 rounded-lg border border-border hover:bg-accent transition-colors cursor-pointer"
                    >
                      <WeatherIcon weather={app.weather || 'cloudy'} className="h-6 w-6 shrink-0" />
                      <div className="flex-1 min-w-0">
                        <p className="font-medium text-sm truncate">{app.name}</p>
                        <p className="text-xs text-muted-foreground truncate">{app.description}</p>
                      </div>

                      {/* Component counts */}
                      <div className="hidden sm:flex items-center gap-2 text-xs">
                        {app.running_count > 0 && (
                          <span className="text-green-600">{app.running_count} running</span>
                        )}
                        {(app.starting_count ?? 0) > 0 && (
                          <span className="text-blue-600 animate-pulse">{app.starting_count} starting</span>
                        )}
                        {(app.stopping_count ?? 0) > 0 && (
                          <span className="text-blue-600 animate-pulse">{app.stopping_count} stopping</span>
                        )}
                        {app.stopped_count > 0 && (
                          <span className="text-gray-500">{app.stopped_count} stopped</span>
                        )}
                        {app.failed_count > 0 && (
                          <span className="text-red-600">{app.failed_count} failed</span>
                        )}
                        {(app.unreachable_count ?? 0) > 0 && (
                          <span className="text-gray-700">{app.unreachable_count} unreachable</span>
                        )}
                      </div>

                      {/* Global state badge */}
                      <Badge variant={getWeatherVariant(app.weather || 'cloudy')} className="shrink-0">
                        {app.global_state || 'UNKNOWN'}
                      </Badge>

                      {/* Action buttons */}
                      <div className="flex items-center gap-1 shrink-0">
                        {/* Show spinner/cancel when:
                            1. Local API call in progress (operatingAppId)
                            2. OR app is in transitional state (STARTING/STOPPING) */}
                        {(operatingAppId === app.id || app.global_state === 'STARTING' || app.global_state === 'STOPPING') ? (
                          <div className="flex items-center gap-2">
                            <div className="flex items-center gap-1.5 text-xs text-muted-foreground px-2">
                              <Loader2 className="h-4 w-4 animate-spin" />
                              <span>
                                {operatingAppId === app.id
                                  ? (operationType === 'start' ? 'Starting...' : 'Stopping...')
                                  : (app.global_state === 'STARTING' ? 'Starting...' : 'Stopping...')}
                              </span>
                            </div>
                            <Button
                              variant="ghost"
                              size="sm"
                              className="h-7 px-2 text-red-600 hover:bg-red-50"
                              onClick={(e) => handleCancel(e, app.id)}
                              title="Cancel operation"
                            >
                              <Ban className="h-3.5 w-3.5 mr-1" />
                              Cancel
                            </Button>
                          </div>
                        ) : (
                          <>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-8 w-8"
                              onClick={(e) => handleStart(e, app.id, app.name)}
                              disabled={startApp.isPending || stopApp.isPending}
                              title="Start all components"
                            >
                              <Play className="h-4 w-4 text-green-600" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-8 w-8"
                              onClick={(e) => handleStop(e, app.id, app.name)}
                              disabled={startApp.isPending || stopApp.isPending}
                              title="Stop all components"
                            >
                              <Square className="h-4 w-4 text-red-600" />
                            </Button>
                          </>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </div>

        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Live Events</CardTitle>
          </CardHeader>
          <CardContent>
            <ScrollArea className="h-[400px]">
              {recentEvents.length === 0 ? (
                <p className="text-sm text-muted-foreground text-center py-8">
                  No recent events
                </p>
              ) : (
                <div className="space-y-2">
                  {recentEvents.map((ev, i) => {
                    const formatted = formatEvent(ev);
                    return (
                      <div key={i} className="text-xs p-2 rounded bg-muted">
                        <div className="flex items-center gap-2">
                          <span className="text-muted-foreground shrink-0">
                            {new Date(ev.timestamp).toLocaleTimeString()}
                          </span>
                          {formatted.icon && (
                            <span className={formatted.color}>{formatted.icon}</span>
                          )}
                          <span className={`font-medium ${formatted.color}`}>
                            {formatted.text}
                          </span>
                        </div>
                        {formatted.context && (
                          <div className="text-muted-foreground ml-[4.5rem] truncate">
                            {formatted.context}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </ScrollArea>
          </CardContent>
        </Card>
      </div>

      <ConfirmDialog
        open={confirmDialog.open}
        onOpenChange={(open) => setConfirmDialog((prev) => ({ ...prev, open }))}
        title={confirmDialog.title}
        description={confirmDialog.description}
        confirmLabel={confirmDialog.confirmLabel}
        variant={confirmDialog.variant}
        onConfirm={confirmDialog.onConfirm}
      />
    </div>
  );
}
