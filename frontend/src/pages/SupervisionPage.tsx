import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { useApps, useApp } from '@/api/apps';
import { useSupervisionStore } from '@/stores/supervision';
import { useFullscreen } from '@/hooks/use-fullscreen';
import { AppMap } from '@/components/maps/AppMap';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Slider } from '@/components/ui/slider';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Checkbox } from '@/components/ui/checkbox';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Settings,
  Maximize,
  Minimize,
  X,
  Sun,
  CloudSun,
  Cloud,
  CloudRain,
  CloudLightning,
  CheckCircle,
  XCircle,
  AlertTriangle,
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

export function SupervisionPage() {
  const navigate = useNavigate();
  const containerRef = useRef<HTMLDivElement>(null);
  const { isFullscreen, toggleFullscreen } = useFullscreen(containerRef);
  const { data: apps } = useApps();

  const {
    selectedAppIds,
    intervalSeconds,
    isPlaying,
    currentIndex,
    toggleAppSelection,
    setIntervalSeconds,
    play,
    pause,
    togglePlay,
    next,
    previous,
    goToIndex,
  } = useSupervisionStore();

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(true);
  const controlsTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Get the apps to display (selected or all)
  const displayApps = useMemo(() => {
    if (!apps) return [];
    if (selectedAppIds.length === 0) return apps;
    return apps.filter((app) => selectedAppIds.includes(app.id));
  }, [apps, selectedAppIds]);

  // Get current app (with wraparound)
  const currentAppIndex = currentIndex % Math.max(1, displayApps.length);
  const currentApp = displayApps[currentAppIndex];

  // Fetch full app data for current app
  const { data: fullAppData } = useApp(currentApp?.id || '');

  // Auto-advance slideshow
  useEffect(() => {
    if (!isPlaying || displayApps.length <= 1) return;

    const timer = setInterval(() => {
      next();
    }, intervalSeconds * 1000);

    return () => clearInterval(timer);
  }, [isPlaying, intervalSeconds, next, displayApps.length]);

  // Auto-hide controls
  useEffect(() => {
    const handleMouseMove = () => {
      setControlsVisible(true);
      if (controlsTimeoutRef.current) {
        clearTimeout(controlsTimeoutRef.current);
      }
      if (isPlaying) {
        controlsTimeoutRef.current = setTimeout(() => {
          setControlsVisible(false);
        }, 3000);
      }
    };

    window.addEventListener('mousemove', handleMouseMove);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      if (controlsTimeoutRef.current) {
        clearTimeout(controlsTimeoutRef.current);
      }
    };
  }, [isPlaying]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;

      switch (e.key) {
        case ' ':
          e.preventDefault();
          togglePlay();
          break;
        case 'ArrowLeft':
          previous();
          break;
        case 'ArrowRight':
          next();
          break;
        case 'f':
        case 'F11':
          e.preventDefault();
          toggleFullscreen();
          break;
        case 'Escape':
          if (!isFullscreen) {
            navigate(-1);
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [togglePlay, previous, next, toggleFullscreen, isFullscreen, navigate]);

  const handleExit = useCallback(() => {
    pause();
    navigate(-1);
  }, [pause, navigate]);

  if (!apps?.length) {
    return (
      <div className="h-screen flex items-center justify-center bg-background">
        <div className="text-center">
          <p className="text-muted-foreground">No applications to display</p>
          <Button variant="outline" className="mt-4" onClick={handleExit}>
            Back
          </Button>
        </div>
      </div>
    );
  }

  const components = fullAppData?.components || [];
  const dependencies = fullAppData?.dependencies || [];

  return (
    <div
      ref={containerRef}
      className="h-screen w-screen bg-background relative overflow-hidden"
    >
      {/* Map display */}
      <div className="absolute inset-0">
        {currentApp && (
          <AppMap
            components={components}
            dependencies={dependencies}
            selectedComponentId={null}
            onSelectComponent={() => {}}
            canOperate={false}
            editable={false}
          />
        )}
      </div>

      {/* Top bar - always visible when controls are shown */}
      <div
        className={`absolute top-0 left-0 right-0 p-4 transition-opacity duration-300 ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="bg-card/90 backdrop-blur border border-border rounded-lg px-4 py-3 shadow-lg flex items-center justify-between">
          {/* App info */}
          <div className="flex items-center gap-3">
            {currentApp && (
              <>
                <WeatherIcon
                  weather={currentApp.weather || 'cloudy'}
                  className="h-6 w-6"
                />
                <h1 className="text-xl font-semibold">{currentApp.name}</h1>
                <Badge variant={getWeatherVariant(currentApp.weather || 'cloudy')}>
                  {currentApp.global_state || 'UNKNOWN'}
                </Badge>
              </>
            )}
          </div>

          {/* Stats */}
          <div className="flex items-center gap-4 text-sm">
            {currentApp && (
              <>
                {currentApp.running_count > 0 && (
                  <span className="flex items-center gap-1 text-green-600">
                    <CheckCircle className="h-4 w-4" />
                    {currentApp.running_count}
                  </span>
                )}
                {currentApp.failed_count > 0 && (
                  <span className="flex items-center gap-1 text-red-600">
                    <XCircle className="h-4 w-4" />
                    {currentApp.failed_count}
                  </span>
                )}
                {currentApp.stopped_count > 0 && (
                  <span className="flex items-center gap-1 text-gray-500">
                    <AlertTriangle className="h-4 w-4" />
                    {currentApp.stopped_count}
                  </span>
                )}
              </>
            )}
          </div>

          {/* Actions */}
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">
              {currentAppIndex + 1} / {displayApps.length}
            </span>

            <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
              <DialogTrigger asChild>
                <Button variant="ghost" size="icon">
                  <Settings className="h-5 w-5" />
                </Button>
              </DialogTrigger>
              <DialogContent className="max-w-md">
                <DialogHeader>
                  <DialogTitle>Supervision Settings</DialogTitle>
                </DialogHeader>
                <div className="space-y-6 py-4">
                  {/* Interval */}
                  <div className="space-y-2">
                    <label className="text-sm font-medium">
                      Rotation interval: {intervalSeconds}s
                    </label>
                    <Slider
                      value={[intervalSeconds]}
                      onValueChange={([v]) => setIntervalSeconds(v)}
                      min={5}
                      max={120}
                      step={5}
                    />
                  </div>

                  {/* App selection */}
                  <div className="space-y-2">
                    <label className="text-sm font-medium">
                      Applications to display
                    </label>
                    <p className="text-xs text-muted-foreground">
                      {selectedAppIds.length === 0
                        ? 'All applications'
                        : `${selectedAppIds.length} selected`}
                    </p>
                    <ScrollArea className="h-48 border rounded-md p-2">
                      <div className="space-y-2">
                        {apps?.map((app) => (
                          <label
                            key={app.id}
                            className="flex items-center gap-2 cursor-pointer hover:bg-accent p-1 rounded"
                          >
                            <Checkbox
                              checked={
                                selectedAppIds.length === 0 ||
                                selectedAppIds.includes(app.id)
                              }
                              onCheckedChange={() => toggleAppSelection(app.id)}
                            />
                            <WeatherIcon
                              weather={app.weather || 'cloudy'}
                              className="h-4 w-4"
                            />
                            <span className="text-sm">{app.name}</span>
                          </label>
                        ))}
                      </div>
                    </ScrollArea>
                  </div>
                </div>
              </DialogContent>
            </Dialog>

            <Button variant="ghost" size="icon" onClick={toggleFullscreen}>
              {isFullscreen ? (
                <Minimize className="h-5 w-5" />
              ) : (
                <Maximize className="h-5 w-5" />
              )}
            </Button>

            <Button variant="ghost" size="icon" onClick={handleExit}>
              <X className="h-5 w-5" />
            </Button>
          </div>
        </div>
      </div>

      {/* Bottom controls */}
      <div
        className={`absolute bottom-0 left-0 right-0 p-4 transition-opacity duration-300 ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="flex justify-center">
          <div className="bg-card/90 backdrop-blur border border-border rounded-full px-6 py-3 shadow-lg flex items-center gap-4">
            <Button
              variant="ghost"
              size="icon"
              onClick={previous}
              disabled={displayApps.length <= 1}
            >
              <SkipBack className="h-5 w-5" />
            </Button>

            <Button
              variant="default"
              size="icon"
              className="h-12 w-12 rounded-full"
              onClick={togglePlay}
            >
              {isPlaying ? (
                <Pause className="h-6 w-6" />
              ) : (
                <Play className="h-6 w-6 ml-0.5" />
              )}
            </Button>

            <Button
              variant="ghost"
              size="icon"
              onClick={next}
              disabled={displayApps.length <= 1}
            >
              <SkipForward className="h-5 w-5" />
            </Button>

            {/* Progress dots */}
            {displayApps.length > 1 && displayApps.length <= 10 && (
              <div className="flex items-center gap-1.5 ml-4">
                {displayApps.map((_, idx) => (
                  <button
                    key={idx}
                    onClick={() => goToIndex(idx)}
                    className={`h-2 rounded-full transition-all ${
                      idx === currentAppIndex
                        ? 'w-4 bg-primary'
                        : 'w-2 bg-muted-foreground/30 hover:bg-muted-foreground/50'
                    }`}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Keyboard shortcuts hint */}
      <div
        className={`absolute bottom-20 left-0 right-0 text-center transition-opacity duration-300 ${
          controlsVisible && !isPlaying ? 'opacity-100' : 'opacity-0'
        }`}
      >
        <p className="text-xs text-muted-foreground">
          Space: Play/Pause • Arrows: Navigate • F: Fullscreen • Esc: Exit
        </p>
      </div>
    </div>
  );
}
