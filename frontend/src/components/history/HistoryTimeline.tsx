import { useState, useCallback, useMemo, useRef, useEffect } from 'react';
import { format, subHours, subDays, parseISO } from 'date-fns';
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
  ChevronDown,
  ChevronUp,
  List,
  ArrowRight,
  Settings2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { Slider } from '@/components/ui/slider';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
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

export type TimeRangePreset = '1h' | '4h' | '24h' | '7d' | '30d' | 'custom';

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

function getPresetRange(preset: TimeRangePreset): { from: Date; to: Date } | null {
  if (preset === 'custom') return null;
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

function getResolutionForRange(from: Date, to: Date): HistoryResolution {
  const durationMs = to.getTime() - from.getTime();
  const hours = durationMs / (1000 * 60 * 60);

  if (hours <= 4) return 'minute';
  if (hours <= 24) return 'fiveminutes';
  if (hours <= 24 * 7) return 'hour';
  return 'day';
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
    return 'bg-purple-500'; // Actions are purple for visibility
  }
  if (event.type === 'command') {
    if (event.exit_code === 0) return 'bg-green-500';
    if (event.exit_code !== null && event.exit_code !== 0) return 'bg-red-500';
    return 'bg-blue-500';
  }
  return 'bg-gray-500';
}

function getEventTextColor(event: HistoryEvent): string {
  if (event.type === 'state_change') {
    const state = event.to_state?.toUpperCase();
    if (state === 'RUNNING') return 'text-green-600';
    if (state === 'FAILED') return 'text-red-600';
    if (state === 'STOPPED') return 'text-gray-500';
    if (state === 'STARTING' || state === 'STOPPING') return 'text-blue-600';
    if (state === 'DEGRADED') return 'text-orange-600';
    return 'text-gray-600';
  }
  if (event.type === 'action') {
    if (event.status === 'failed') return 'text-red-600';
    if (event.status === 'success') return 'text-green-600';
    return 'text-purple-600';
  }
  if (event.type === 'command') {
    if (event.exit_code === 0) return 'text-green-600';
    if (event.exit_code !== null && event.exit_code !== 0) return 'text-red-600';
    return 'text-blue-600';
  }
  return 'text-gray-600';
}

function getEventLabel(event: HistoryEvent): string {
  if (event.type === 'state_change') {
    return `${event.component_name}: ${event.from_state} → ${event.to_state}`;
  }
  if (event.type === 'action') {
    const target = event.component_name ? ` on ${event.component_name}` : '';
    const actionName = formatActionName(event.action || '');
    return `${actionName}${target}`;
  }
  if (event.type === 'command') {
    return `${event.command_type} on ${event.component_name}`;
  }
  return 'Event';
}

function formatActionName(action: string): string {
  // Make action names more readable
  return action
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function getEventDetails(event: HistoryEvent): string | null {
  if (event.type === 'action' && event.details) {
    const details = event.details as Record<string, unknown>;
    if (details.mode) return `Mode: ${details.mode}`;
    if (details.target_site) return `Target site`;
  }
  if (event.type === 'state_change' && event.trigger) {
    return `Trigger: ${event.trigger}`;
  }
  return null;
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

  // Determine format based on duration
  const durationHours = (to.getTime() - from.getTime()) / (1000 * 60 * 60);
  const timeFormat = durationHours > 24 ? 'MMM d HH:mm' : 'HH:mm';

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
            {format(time, timeFormat)}
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Event List Item Component
// ---------------------------------------------------------------------------

interface EventListItemProps {
  event: HistoryEvent;
  onClick: () => void;
  isSelected: boolean;
}

function EventListItem({ event, onClick, isSelected }: EventListItemProps) {
  const colorClass = getEventColor(event);
  const textColorClass = getEventTextColor(event);
  const details = getEventDetails(event);

  return (
    <button
      onClick={onClick}
      className={cn(
        'w-full text-left px-3 py-2 rounded-md transition-colors',
        'hover:bg-muted/50',
        isSelected && 'bg-muted ring-1 ring-primary'
      )}
    >
      <div className="flex items-start gap-3">
        {/* Color indicator */}
        <div className={cn('w-2 h-2 rounded-full mt-1.5 flex-shrink-0', colorClass)} />

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            {getEventIcon(event)}
            <span className={cn('text-sm font-medium', textColorClass)}>
              {getEventLabel(event)}
            </span>
          </div>
          <div className="flex items-center gap-2 mt-0.5">
            <span className="text-xs text-muted-foreground">
              {format(new Date(event.at), 'HH:mm:ss')}
            </span>
            {event.type === 'action' && event.user && (
              <span className="text-xs text-muted-foreground">
                by {event.user}
              </span>
            )}
            {details && (
              <span className="text-xs text-muted-foreground">
                • {details}
              </span>
            )}
          </div>
        </div>

        {/* Arrow */}
        <ArrowRight className="h-4 w-4 text-muted-foreground flex-shrink-0 mt-1" />
      </div>
    </button>
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
  const [customFrom, setCustomFrom] = useState<string>('');
  const [customTo, setCustomTo] = useState<string>('');
  const [isPlaying, setIsPlaying] = useState(false);
  const [sliderValue, setSliderValue] = useState<number[]>([100]); // 0-100
  const [showEventList, setShowEventList] = useState(true);
  const [selectedEventIdx, setSelectedEventIdx] = useState<number | null>(null);
  const playIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Drag-to-select state
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState<number | null>(null);
  const [dragEnd, setDragEnd] = useState<number | null>(null);
  const timelineRef = useRef<HTMLDivElement>(null);

  // Compute time range from preset or custom
  const timeRange = useMemo(() => {
    if (preset === 'custom' && customFrom && customTo) {
      try {
        const from = parseISO(customFrom);
        const to = parseISO(customTo);
        if (from < to) return { from, to };
      } catch {
        // Invalid dates, fall back
      }
    }
    return getPresetRange(preset) || { from: subHours(new Date(), 4), to: new Date() };
  }, [preset, customFrom, customTo]);

  const resolution = useMemo(() => getResolutionForRange(timeRange.from, timeRange.to), [timeRange]);

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
      setSelectedEventIdx(null);
      const time = getTimeForPosition(value[0]);
      const snapshot = findSnapshotForTime(time);
      onSelectTime(time, snapshot);
    },
    [getTimeForPosition, findSnapshotForTime, onSelectTime],
  );

  // Handle event click
  const handleEventClick = useCallback(
    (event: HistoryEvent, idx: number) => {
      const eventTime = new Date(event.at);
      const position = getPositionForTime(eventTime);
      setSliderValue([position]);
      setSelectedEventIdx(idx);
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
    const currentTime = getTimeForPosition(sliderValue[0]);
    const events = [...history.events].reverse();
    for (let i = 0; i < events.length; i++) {
      const event = events[i];
      const eventTime = new Date(event.at);
      if (eventTime < currentTime) {
        handleEventClick(event, history.events.length - 1 - i);
        return;
      }
    }
    setSliderValue([0]);
    setSelectedEventIdx(null);
    onSelectTime(timeRange.from, findSnapshotForTime(timeRange.from));
  }, [history, sliderValue, getTimeForPosition, handleEventClick, timeRange, findSnapshotForTime, onSelectTime]);

  const handleSkipForward = useCallback(() => {
    if (!history?.events.length) return;
    const currentTime = getTimeForPosition(sliderValue[0]);
    for (let i = 0; i < history.events.length; i++) {
      const event = history.events[i];
      const eventTime = new Date(event.at);
      if (eventTime > currentTime) {
        handleEventClick(event, i);
        return;
      }
    }
    setSliderValue([100]);
    setSelectedEventIdx(null);
    onSelectTime(timeRange.to, findSnapshotForTime(timeRange.to));
  }, [history, sliderValue, getTimeForPosition, handleEventClick, timeRange, findSnapshotForTime, onSelectTime]);

  // Playback effect
  useEffect(() => {
    if (isPlaying && history?.events.length) {
      playIntervalRef.current = setInterval(() => {
        setSliderValue((prev) => {
          const newValue = prev[0] + 0.5;
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

  // Handle preset change
  const handlePresetChange = useCallback((newPreset: TimeRangePreset) => {
    setPreset(newPreset);
    setSliderValue([100]);
    setIsPlaying(false);
    setSelectedEventIdx(null);
  }, []);

  // Handle custom range apply
  const handleApplyCustomRange = useCallback(() => {
    if (customFrom && customTo) {
      setPreset('custom');
      setSliderValue([100]);
      setIsPlaying(false);
      setSelectedEventIdx(null);
    }
  }, [customFrom, customTo]);

  // Drag-to-select handlers
  const getPositionFromMouseEvent = useCallback((e: React.MouseEvent | MouseEvent): number => {
    if (!timelineRef.current) return 0;
    const rect = timelineRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    return Math.max(0, Math.min(100, (x / rect.width) * 100));
  }, []);

  const handleTimelineMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const position = getPositionFromMouseEvent(e);
    setIsDragging(true);
    setDragStart(position);
    setDragEnd(position);
    setIsPlaying(false);
  }, [getPositionFromMouseEvent]);

  const handleTimelineMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging) return;
    const position = getPositionFromMouseEvent(e);
    setDragEnd(position);
  }, [isDragging, getPositionFromMouseEvent]);

  const handleTimelineMouseUp = useCallback(() => {
    if (!isDragging || dragStart === null || dragEnd === null) {
      setIsDragging(false);
      return;
    }

    const startPos = Math.min(dragStart, dragEnd);
    const endPos = Math.max(dragStart, dragEnd);

    // If it's just a click (not a drag), select that point
    if (Math.abs(endPos - startPos) < 2) {
      const clickTime = getTimeForPosition(startPos);
      const snapshot = findSnapshotForTime(clickTime);
      setSliderValue([startPos]);
      setSelectedEventIdx(null);
      onSelectTime(clickTime, snapshot);
    } else {
      // It's a drag - apply as custom range
      const fromTime = getTimeForPosition(startPos);
      const toTime = getTimeForPosition(endPos);
      setCustomFrom(fromTime.toISOString().slice(0, 16));
      setCustomTo(toTime.toISOString().slice(0, 16));
      setPreset('custom');
      setSliderValue([100]); // Reset to end of new range
    }

    setIsDragging(false);
    setDragStart(null);
    setDragEnd(null);
  }, [isDragging, dragStart, dragEnd, getTimeForPosition, findSnapshotForTime, onSelectTime]);

  // Handle mouse leave during drag
  const handleTimelineMouseLeave = useCallback(() => {
    if (isDragging) {
      handleTimelineMouseUp();
    }
  }, [isDragging, handleTimelineMouseUp]);

  // Compute drag selection range for visual feedback
  const dragSelectionRange = useMemo(() => {
    if (!isDragging || dragStart === null || dragEnd === null) return null;
    const left = Math.min(dragStart, dragEnd);
    const width = Math.abs(dragEnd - dragStart);
    return { left, width };
  }, [isDragging, dragStart, dragEnd]);

  // Current selected time display
  const currentTime = useMemo(() => getTimeForPosition(sliderValue[0]), [sliderValue, getTimeForPosition]);

  // Sort events by time (newest first for list)
  const sortedEvents = useMemo(() => {
    if (!history?.events) return [];
    return [...history.events].reverse();
  }, [history]);

  return (
    <div className={cn('bg-card border-t border-border', className)}>
      {/* Header */}
      <div className="flex items-center justify-between p-4 pb-3">
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
            {/* Custom range picker */}
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  variant={preset === 'custom' ? 'default' : 'ghost'}
                  size="sm"
                  className="h-7 px-2 text-xs gap-1"
                >
                  <Settings2 className="h-3 w-3" />
                  Custom
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-80" align="start">
                <div className="space-y-4">
                  <div className="space-y-2">
                    <Label htmlFor="custom-from">From</Label>
                    <Input
                      id="custom-from"
                      type="datetime-local"
                      value={customFrom}
                      onChange={(e) => setCustomFrom(e.target.value)}
                      className="text-sm"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="custom-to">To</Label>
                    <Input
                      id="custom-to"
                      type="datetime-local"
                      value={customTo}
                      onChange={(e) => setCustomTo(e.target.value)}
                      className="text-sm"
                    />
                  </div>
                  <Button
                    onClick={handleApplyCustomRange}
                    disabled={!customFrom || !customTo}
                    className="w-full"
                    size="sm"
                  >
                    Apply Range
                  </Button>
                </div>
              </PopoverContent>
            </Popover>
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

        {/* Right: Current time display + toggle */}
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
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0 ml-2"
            onClick={() => setShowEventList((prev) => !prev)}
            title={showEventList ? 'Hide event list' : 'Show event list'}
          >
            <List className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {/* Timeline */}
      <div className="relative px-4">
        {/* Events track - now interactive for drag-to-select */}
        <div
          ref={timelineRef}
          className="relative h-6 bg-muted/50 rounded-md cursor-crosshair select-none"
          onMouseDown={handleTimelineMouseDown}
          onMouseMove={handleTimelineMouseMove}
          onMouseUp={handleTimelineMouseUp}
          onMouseLeave={handleTimelineMouseLeave}
        >
          {/* Drag selection highlight */}
          {dragSelectionRange && (
            <div
              className="absolute top-0 bottom-0 bg-primary/20 border-x border-primary pointer-events-none"
              style={{
                left: `${dragSelectionRange.left}%`,
                width: `${dragSelectionRange.width}%`,
              }}
            />
          )}

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
                onClick={() => handleEventClick(event, idx)}
                isSelected={selectedEventIdx === idx || Math.abs(position - sliderValue[0]) < 1}
              />
            );
          })}

          {/* Current position indicator */}
          {!isDragging && (
            <div
              className="absolute top-0 bottom-0 w-0.5 bg-primary pointer-events-none"
              style={{ left: `${sliderValue[0]}%` }}
            />
          )}
        </div>

        {/* Slider (hidden visually but provides interaction) */}
        <div className="absolute inset-x-4 top-0 h-6">
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
        <div className="flex items-center justify-between px-4 py-2 text-xs text-muted-foreground">
          <span>
            {history.events.length} events in selected range
          </span>
          <span>
            {history.snapshots.length} snapshots
          </span>
        </div>
      )}

      {/* Event List */}
      <Collapsible open={showEventList} onOpenChange={setShowEventList}>
        <CollapsibleTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="w-full h-8 rounded-none border-t flex items-center justify-center gap-2 text-xs text-muted-foreground hover:text-foreground"
          >
            {showEventList ? (
              <>
                <ChevronDown className="h-3 w-3" />
                Hide Events
              </>
            ) : (
              <>
                <ChevronUp className="h-3 w-3" />
                Show Events ({sortedEvents.length})
              </>
            )}
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          {sortedEvents.length > 0 ? (
            <ScrollArea className="h-[200px] border-t">
              <div className="p-2 space-y-1">
                {sortedEvents.map((event, idx) => {
                  const originalIdx = history!.events.length - 1 - idx;
                  return (
                    <EventListItem
                      key={`${event.type}-${event.at}-${idx}`}
                      event={event}
                      onClick={() => handleEventClick(event, originalIdx)}
                      isSelected={selectedEventIdx === originalIdx}
                    />
                  );
                })}
              </div>
            </ScrollArea>
          ) : (
            <div className="p-4 text-center text-sm text-muted-foreground border-t">
              No events in selected time range
            </div>
          )}
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
