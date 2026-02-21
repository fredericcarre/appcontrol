import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { useApps } from '@/api/apps';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useWebSocketStore } from '@/stores/websocket';
import {
  Sun, CloudSun, Cloud, CloudRain, CloudLightning,
  Plus, Activity, AlertTriangle, CheckCircle, XCircle,
} from 'lucide-react';

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

export function DashboardPage() {
  const { data: apps, isLoading } = useApps();
  const messages = useWebSocketStore((s) => s.messages);
  const navigate = useNavigate();

  const stats = useMemo(() => {
    if (!apps) return { total: 0, healthy: 0, degraded: 0, failed: 0 };
    return {
      total: apps.length,
      healthy: apps.filter((a) => a.weather === 'sunny' || a.weather === 'fair').length,
      degraded: apps.filter((a) => a.weather === 'cloudy' || a.weather === 'rainy').length,
      failed: apps.filter((a) => a.weather === 'stormy').length,
    };
  }, [apps]);

  const recentEvents = messages.slice(-20).reverse();

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

      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
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
            <CheckCircle className="h-8 w-8 text-state-running" />
            <div>
              <p className="text-2xl font-bold">{stats.healthy}</p>
              <p className="text-xs text-muted-foreground">Healthy</p>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 flex items-center gap-3">
            <AlertTriangle className="h-8 w-8 text-state-degraded" />
            <div>
              <p className="text-2xl font-bold">{stats.degraded}</p>
              <p className="text-xs text-muted-foreground">Degraded</p>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 flex items-center gap-3">
            <XCircle className="h-8 w-8 text-state-failed" />
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
                    <button
                      key={app.id}
                      onClick={() => navigate(`/apps/${app.id}`)}
                      className="w-full flex items-center gap-3 p-3 rounded-lg border border-border hover:bg-accent transition-colors text-left"
                    >
                      <WeatherIcon weather={app.weather || 'cloudy'} className="h-6 w-6 shrink-0" />
                      <div className="flex-1 min-w-0">
                        <p className="font-medium text-sm truncate">{app.name}</p>
                        <p className="text-xs text-muted-foreground truncate">{app.description}</p>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge variant={getWeatherVariant(app.weather || 'cloudy')}>
                          {app.weather || 'unknown'}
                        </Badge>
                        <span className="text-xs text-muted-foreground">
                          {app.component_count} components
                        </span>
                      </div>
                    </button>
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
                  {recentEvents.map((ev, i) => (
                    <div key={i} className="text-xs p-2 rounded bg-muted">
                      <span className="text-muted-foreground">
                        {new Date(ev.timestamp).toLocaleTimeString()}
                      </span>
                      {' '}
                      <span className="font-medium">{ev.type}</span>
                    </div>
                  ))}
                </div>
              )}
            </ScrollArea>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
