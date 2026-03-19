import { useState, useCallback, useMemo, useRef, useEffect } from 'react';
import { format, subHours, subDays } from 'date-fns';
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Clock,
  Calendar,
  Zap,
  User,
  Terminal,
  AlertCircle,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { Slider } from '@/components/ui/slider';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import {
  useAppHistory,
  HistoryEvent,
  TimeSnapshot,
  HistoryResolution,
  HistoryQueryParams,
} from '@/api/apps';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type TimeRangePreset = '1h' | '4h' | '24h' | '7d' | '30d';

interface HistoryTimelineProps {
  appId: string;
  onSelectTime: (time: Date, snapshot: TimeSnapshot | null) => void;
  className?: string;
}

interface EventMarkerProps {
  event: HistoryEvent;
  position: number; // 0-100 percentage
  onClick: () => void;
  isSelected: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getPresetRange(preset: TimeRangePreset): { from: Date; to: Date } {
  const now = new Date();
  switch (preset) {
    case '1h':
      return { from: subHours(now, 1), to: now };
    case '4h':
      return { from: subHours(now, 4), to: now };
    case '24h':
      return { from: subDays(now, 1), to: now };
    case '7d':
      return { from: subDays(now, 7), to: now };
    case '30d':
      return { from: subDays(now, 30), to: now };
  }
}

function getResolutionForPreset(preset: TimeRangePreset): HistoryResolution {
  switch (preset) {
    case '1h':
    case '4h':
      return 'minute';
    case '24h':
      return 'fiveminutes';
    case '7d':
      return 'hour';
    case '30d':
      return 'day';
  }
}

function getEventIcon(event: HistoryEvent) {
  switch (event.type) {
    case 'state_change':
      return <Zap className="h-3 w-3" />;
    case 'action':
      return <User className="h-3 w-3" />;
    case 'command':
      return <Terminal className="h-3 w-3" />;
    default:
      return <Clock className="h-3 w-3" />;
  }
}

function getEventColor(event: HistoryEvent): string {
  if (event.type === 'state_change') {
    const state = event.to_state?.toUpperCase();
    if (state === 'RUNNING') return 'bg-green-500';
    if (state === 'FAILED') return 'bg-red-500';
    if (state === 'STOPPED') return 'bg-gray-400';
    if (state === 'STARTING' || state === 'STOPPING') return 'bg-blue-500';
    if (state === 'DEGRADED') return 'bg-orange-500';
    return 'bg-gray-500';
  }
  if (event.type === 'action') {
    if (event.status === 'failed') return 'bg-red-500';
    if (event.status === 'success') return 'bg-green-500';
    return 'bg-blue-500';
  }
  if (event.type === 'command') {
    if (event.exit_code === 0) return 'bg-green-500';
    if (event.exit_code !== null && event.exit_code !== 0) return 'bg-red-500';
    return 'bg-blue-500';
  }
  return 'bg-gray-500';
}

function getEventLabel(event: HistoryEvent): string {
  if (event.type === 'state_change') {
    return `${event.component_name}: ${event.from_state} → ${event.to_state}`;
  }
  if (event.type === 'action') {
    const target = event.component_name ? ` on ${event.component_name}` : '';
    return `${event.user}: ${event.action}${target}`;
  }
  if (event.type === 'command') {
    return `${event.command_type} on ${event.component_name}`;
  }
  return 'Event';
}

// ---------------------------------------------------------------------------
// Event Marker Component
// ---------------------------------------------------------------------------

function EventMarker({ event, position, onClick, isSelected }: EventMarkerProps) {
  const colorClass = getEventColor(event);

  return (
    <TooltipProvider>
      <Tooltip delayDuration={100}>
        <TooltipTrigger asChild>
          <button
            onClick={onClick}
            className={cn(
              'absolute top-1/2 -translate-y-1/2 w-3 h-3 rounded-full cursor-pointer',
              'hover:ring-2 hover:ring-offset-1 hover:ring-primary',
              'transition-all duration-150',
              colorClass,
              isSelected && 'ring-2 ring-offset-1 ring-primary scale-125',
            )}
            style={{ left: `${position}%` }}
          />
        </TooltipTrigger>
        <TooltipContent side="top" className="max-w-xs">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              {getEventIcon(event)}
              <span className="font-medium capitalize">{event.type.replace('_', ' ')}</span>
            </div>
            <p className="text-sm">{getEventLabel(event)}</p>
            <p className="text-xs text-muted-foreground">
              {format(new Date(event.at), 'PPpp')}
            </p>
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

// ---------------------------------------------------------------------------
// Time Axis Component
// ---------------------------------------------------------------------------

interface TimeAxisProps {
  from: Date;
  to: Date;
}

function TimeAxis({ from, to }: TimeAxisProps) {
  const ticks = useMemo(() => {
    const duration = to.getTime() - from.getTime();
    const tickCount = 5;
    const result = [];
    for (let i = 0; i <= tickCount; i++) {
      const time = new Date(from.getTime() + (duration * i) / tickCount);
      const position = (i / tickCount) * 100;
      result.push({ time, position });
    }
    return result;
  }, [from, to]);

  return (
    <div className="relative h-5 mt-1">
      {ticks.map(({ time, position }, idx) => (
        <div
          key={idx}
          className="absolute flex flex-col items-center"
          style={{ left: `${position}%`, transform: 'translateX(-50%)' }}
        >
          <div className="w-px h-2 bg-border" />
          <span className="text-[10px] text-muted-foreground whitespace-nowrap">
            {format(time, 'HH:mm')}
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

export function HistoryTimeline({
  appId,
  onSelectTime,
  className,
}: HistoryTimelineProps) {
  // State
  const [preset, setPreset] = useState<TimeRangePreset>('4h');
  const [isPlaying, setIsPlaying] = useState(false);
  const [sliderValue, setSliderValue] = useState<number[]>([100]); // 0-100
  const playIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Compute time range from preset
  const timeRange = useMemo(() => getPresetRange(preset), [preset]);
  const resolution = useMemo(() => getResolutionForPreset(preset), [preset]);

  // Query params
  const queryParams: HistoryQueryParams = useMemo(
    () => ({
      from: timeRange.from,
      to: timeRange.to,
      resolution,
      eventLimit: 500,
    }),
    [timeRange, resolution],
  );

  // Fetch history
  const { data: history, isLoading, isError } = useAppHistory(appId, queryParams);

  // Calculate position on timeline (0-100) for a given time
  const getPositionForTime = useCallback(
    (time: Date): number => {
      const totalDuration = timeRange.to.getTime() - timeRange.from.getTime();
      const elapsed = time.getTime() - timeRange.from.getTime();
      return Math.max(0, Math.min(100, (elapsed / totalDuration) * 100));
    },
    [timeRange],
  );

  // Calculate time for a given position (0-100)
  const getTimeForPosition = useCallback(
    (position: number): Date => {
      const totalDuration = timeRange.to.getTime() - timeRange.from.getTime();
      return new Date(timeRange.from.getTime() + (totalDuration * position) / 100);
    },
    [timeRange],
  );

  // Find snapshot for a given time
  const findSnapshotForTime = useCallback(
    (time: Date): TimeSnapshot | null => {
      if (!history?.snapshots.length) return null;
      // Find the snapshot closest to but not after the given time
      let closest: TimeSnapshot | null = null;
      for (const snap of history.snapshots) {
        const snapTime = new Date(snap.at);
        if (snapTime <= time) {
          closest = snap;
        } else {
          break;
        }
      }
      return closest;
    },
    [history],
  );

  // Handle slider change
  const handleSliderChange = useCallback(
    (value: number[]) => {
      setSliderValue(value);
      const time = getTimeForPosition(value[0]);
      const snapshot = findSnapshotForTime(time);
      onSelectTime(time, snapshot);
    },
    [getTimeForPosition, findSnapshotForTime, onSelectTime],
  );

  // Handle event click
  const handleEventClick = useCallback(
    (event: HistoryEvent) => {
      const eventTime = new Date(event.at);
      const position = getPositionForTime(eventTime);
      setSliderValue([position]);
      const snapshot = findSnapshotForTime(eventTime);
      onSelectTime(eventTime, snapshot);
    },
    [getPositionForTime, findSnapshotForTime, onSelectTime],
  );

  // Playback controls
  const handlePlayPause = useCallback(() => {
    setIsPlaying((prev) => !prev);
  }, []);

  const handleSkipBack = useCallback(() => {
    if (!history?.events.length) return;
    // Find previous event
    const currentTime = getTimeForPosition(sliderValue[0]);
    const events = [...history.events].reverse(); // Oldest first
    for (const event of events) {
      const eventTime = new Date(event.at);
      if (eventTime < currentTime) {
        handleEventClick(event);
        return;
      }
    }
    // Go to start
    setSliderValue([0]);
    onSelectTime(timeRange.from, findSnapshotForTime(timeRange.from));
  }, [history, sliderValue, getTimeForPosition, handleEventClick, timeRange, findSnapshotForTime, onSelectTime]);

  const handleSkipForward = useCallback(() => {
    if (!history?.events.length) return;
    // Find next event
    const currentTime = getTimeForPosition(sliderValue[0]);
    for (const event of history.events) {
      const eventTime = new Date(event.at);
      if (eventTime > currentTime) {
        handleEventClick(event);
        return;
      }
    }
    // Go to end
    setSliderValue([100]);
    onSelectTime(timeRange.to, findSnapshotForTime(timeRange.to));
  }, [history, sliderValue, getTimeForPosition, handleEventClick, timeRange, findSnapshotForTime, onSelectTime]);

  // Playback effect
  useEffect(() => {
    if (isPlaying && history?.events.length) {
      playIntervalRef.current = setInterval(() => {
        setSliderValue((prev) => {
          const newValue = prev[0] + 0.5; // Advance 0.5% per tick
          if (newValue >= 100) {
            setIsPlaying(false);
            return [100];
          }
          const time = getTimeForPosition(newValue);
          const snapshot = findSnapshotForTime(time);
          onSelectTime(time, snapshot);
          return [newValue];
        });
      }, 100);
    } else if (playIntervalRef.current) {
      clearInterval(playIntervalRef.current);
      playIntervalRef.current = null;
    }

    return () => {
      if (playIntervalRef.current) {
        clearInterval(playIntervalRef.current);
      }
    };
  }, [isPlaying, history, getTimeForPosition, findSnapshotForTime, onSelectTime]);

  // Handle preset change - reset slider and stop playback
  const handlePresetChange = useCallback((newPreset: TimeRangePreset) => {
    setPreset(newPreset);
    setSliderValue([100]);
    setIsPlaying(false);
  }, []);

  // Current selected time display
  const currentTime = useMemo(() => getTimeForPosition(sliderValue[0]), [sliderValue, getTimeForPosition]);

  return (
    <div className={cn('bg-card border-t border-border p-4', className)}>
      <div className="flex items-center justify-between mb-3">
        {/* Left: Time range selector */}
        <div className="flex items-center gap-2">
          <Calendar className="h-4 w-4 text-muted-foreground" />
          <div className="flex gap-1">
            {(['1h', '4h', '24h', '7d', '30d'] as TimeRangePreset[]).map((p) => (
              <Button
                key={p}
                variant={preset === p ? 'default' : 'ghost'}
                size="sm"
                className="h-7 px-2 text-xs"
                onClick={() => handlePresetChange(p)}
              >
                {p}
              </Button>
            ))}
          </div>
        </div>

        {/* Center: Playback controls */}
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={handleSkipBack}
            disabled={isLoading || !history?.events.length}
          >
            <SkipBack className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-8 w-8 p-0"
            onClick={handlePlayPause}
            disabled={isLoading || !history?.events.length}
          >
            {isPlaying ? (
              <Pause className="h-5 w-5" />
            ) : (
              <Play className="h-5 w-5" />
            )}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={handleSkipForward}
            disabled={isLoading || !history?.events.length}
          >
            <SkipForward className="h-4 w-4" />
          </Button>
        </div>

        {/* Right: Current time display */}
        <div className="flex items-center gap-2 text-sm">
          <Clock className="h-4 w-4 text-muted-foreground" />
          <span className="font-mono">
            {format(currentTime, 'MMM d, HH:mm:ss')}
          </span>
          {sliderValue[0] >= 99.5 && (
            <Badge variant="secondary" className="text-xs">
              Live
            </Badge>
          )}
        </div>
      </div>

      {/* Timeline */}
      <div className="relative">
        {/* Events track */}
        <div className="relative h-6 bg-muted/50 rounded-md">
          {/* Event markers */}
          {history?.events.map((event, idx) => {
            const eventTime = new Date(event.at);
            const position = getPositionForTime(eventTime);
            if (position < 0 || position > 100) return null;
            return (
              <EventMarker
                key={`${event.type}-${event.at}-${idx}`}
                event={event}
                position={position}
                onClick={() => handleEventClick(event)}
                isSelected={Math.abs(position - sliderValue[0]) < 1}
              />
            );
          })}

          {/* Current position indicator */}
          <div
            className="absolute top-0 bottom-0 w-0.5 bg-primary"
            style={{ left: `${sliderValue[0]}%` }}
          />
        </div>

        {/* Slider (hidden visually but provides interaction) */}
        <div className="absolute inset-x-0 top-0 h-6">
          <Slider
            value={sliderValue}
            onValueChange={handleSliderChange}
            max={100}
            step={0.1}
            className="h-full opacity-0 cursor-pointer"
          />
        </div>

        {/* Time axis */}
        <TimeAxis from={timeRange.from} to={timeRange.to} />
      </div>

      {/* Loading / Error states */}
      {isLoading && (
        <div className="flex items-center justify-center py-2 text-sm text-muted-foreground">
          Loading history...
        </div>
      )}
      {isError && (
        <div className="flex items-center justify-center py-2 text-sm text-destructive">
          <AlertCircle className="h-4 w-4 mr-2" />
          Failed to load history
        </div>
      )}

      {/* Event count */}
      {history && !isLoading && (
        <div className="flex items-center justify-between mt-2 text-xs text-muted-foreground">
          <span>
            {history.events.length} events in selected range
          </span>
          <span>
            {history.snapshots.length} snapshots
          </span>
        </div>
      )}
    </div>
  );
}
