import { useState, useMemo } from 'react';
import {
  FileText,
  Monitor,
  Terminal,
  Plus,
  Search,
  Clock,
  RefreshCw,
  ChevronDown,
  AlertCircle,
  Trash2,
  ToggleLeft,
  ToggleRight,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  useComponentLogSources,
  useComponentLogs,
  useCreateLogSource,
  useDeleteLogSource,
  useUpdateLogSource,
  getLevelBadgeColor,
  type LogSource,
  type LogEntry,
} from '@/api/logs';
import { cn } from '@/lib/utils';

interface LogSourcesPanelProps {
  componentId: string;
  componentName?: string;
}

const TIME_RANGES = [
  { value: '15m', label: 'Last 15 min' },
  { value: '1h', label: 'Last hour' },
  { value: '6h', label: 'Last 6 hours' },
  { value: '24h', label: 'Last 24 hours' },
  { value: '7d', label: 'Last 7 days' },
];

const LINE_COUNTS = [
  { value: '50', label: '50 lines' },
  { value: '100', label: '100 lines' },
  { value: '200', label: '200 lines' },
  { value: '500', label: '500 lines' },
];

export function LogSourcesPanel({
  componentId,
  componentName,
}: LogSourcesPanelProps) {
  // State
  const [selectedSource, setSelectedSource] = useState<string>('process');
  const [filter, setFilter] = useState('');
  const [timeRange, setTimeRange] = useState('1h');
  const [lineCount, setLineCount] = useState('100');
  const [showAddDialog, setShowAddDialog] = useState(false);

  // Queries
  const { data: sources = [], isLoading: sourcesLoading } =
    useComponentLogSources(componentId);
  const {
    data: logsData,
    isLoading: logsLoading,
    refetch: refetchLogs,
    isFetching,
  } = useComponentLogs(componentId, {
    source: selectedSource,
    lines: parseInt(lineCount),
    filter: filter || undefined,
    since: timeRange,
  });

  // Mutations
  const deleteSource = useDeleteLogSource();
  const updateSource = useUpdateLogSource();

  // Source options for dropdown
  const sourceOptions = useMemo(() => {
    const options = [
      { value: 'process', label: 'Process Output', icon: Terminal },
    ];
    sources.forEach((s) => {
      options.push({
        value: s.id,
        label: s.name,
        icon: s.source_type === 'file' ? FileText : Monitor,
      });
    });
    return options;
  }, [sources]);

  const selectedSourceData = useMemo(() => {
    if (selectedSource === 'process') return null;
    return sources.find((s) => s.id === selectedSource);
  }, [selectedSource, sources]);

  const handleDeleteSource = async (source: LogSource) => {
    if (
      confirm(`Delete log source "${source.name}"? This cannot be undone.`)
    ) {
      await deleteSource.mutateAsync({ id: source.id, componentId });
      if (selectedSource === source.id) {
        setSelectedSource('process');
      }
    }
  };

  const handleToggleSource = async (source: LogSource) => {
    await updateSource.mutateAsync({
      id: source.id,
      componentId,
      is_enabled: !source.is_enabled,
    });
  };

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex flex-col gap-2 p-2 border-b bg-muted/30">
        {/* Source selector row */}
        <div className="flex items-center gap-2">
          <Select value={selectedSource} onValueChange={setSelectedSource}>
            <SelectTrigger className="flex-1 h-8 text-xs">
              <SelectValue placeholder="Select source" />
            </SelectTrigger>
            <SelectContent>
              {sourceOptions.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  <div className="flex items-center gap-2">
                    <opt.icon className="h-3 w-3" />
                    <span>{opt.label}</span>
                  </div>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            size="sm"
            className="h-8 px-2"
            onClick={() => setShowAddDialog(true)}
            title="Add log source"
          >
            <Plus className="h-3 w-3" />
          </Button>
        </div>

        {/* Filters row */}
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground" />
            <Input
              placeholder="Filter logs..."
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="h-8 pl-7 text-xs"
            />
          </div>
          <Select value={timeRange} onValueChange={setTimeRange}>
            <SelectTrigger className="w-24 h-8 text-xs">
              <Clock className="h-3 w-3 mr-1" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TIME_RANGES.map((tr) => (
                <SelectItem key={tr.value} value={tr.value}>
                  {tr.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={lineCount} onValueChange={setLineCount}>
            <SelectTrigger className="w-20 h-8 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {LINE_COUNTS.map((lc) => (
                <SelectItem key={lc.value} value={lc.value}>
                  {lc.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="ghost"
            size="sm"
            className="h-8 px-2"
            onClick={() => refetchLogs()}
            disabled={isFetching}
          >
            <RefreshCw
              className={cn('h-3 w-3', isFetching && 'animate-spin')}
            />
          </Button>
        </div>

        {/* Source info/actions */}
        {selectedSourceData && (
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span className="truncate">
              {selectedSourceData.source_type === 'file'
                ? selectedSourceData.file_path
                : selectedSourceData.log_name}
            </span>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm" className="h-6 px-1">
                  <ChevronDown className="h-3 w-3" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  onClick={() => handleToggleSource(selectedSourceData)}
                >
                  {selectedSourceData.is_enabled ? (
                    <>
                      <ToggleRight className="h-4 w-4 mr-2" />
                      Disable
                    </>
                  ) : (
                    <>
                      <ToggleLeft className="h-4 w-4 mr-2" />
                      Enable
                    </>
                  )}
                </DropdownMenuItem>
                <DropdownMenuItem
                  className="text-destructive"
                  onClick={() => handleDeleteSource(selectedSourceData)}
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )}
      </div>

      {/* Log entries */}
      <ScrollArea className="flex-1">
        {logsLoading || sourcesLoading ? (
          <div className="flex items-center justify-center h-32 text-muted-foreground">
            <RefreshCw className="h-4 w-4 animate-spin mr-2" />
            Loading logs...
          </div>
        ) : !logsData || logsData.entries.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-muted-foreground">
            <AlertCircle className="h-5 w-5 mb-2" />
            <span className="text-sm">No log entries found</span>
            <span className="text-xs">
              {filter
                ? 'Try adjusting your filter'
                : 'Waiting for log data...'}
            </span>
          </div>
        ) : (
          <div className="p-2">
            {logsData.truncated && (
              <div className="text-xs text-muted-foreground text-center mb-2 py-1 bg-muted/50 rounded">
                Showing latest {logsData.entries.length} of{' '}
                {logsData.total_lines} lines
              </div>
            )}
            <div className="space-y-1 font-mono text-xs">
              {logsData.entries.map((entry, idx) => (
                <LogEntryRow key={idx} entry={entry} />
              ))}
            </div>
          </div>
        )}
      </ScrollArea>

      {/* Add source dialog */}
      <AddLogSourceDialog
        open={showAddDialog}
        onOpenChange={setShowAddDialog}
        componentId={componentId}
        componentName={componentName}
        onSuccess={() => setShowAddDialog(false)}
      />
    </div>
  );
}

// ── Log Entry Row ──────────────────────────────────────────────

function LogEntryRow({ entry }: { entry: LogEntry }) {
  const timestamp = entry.timestamp
    ? new Date(entry.timestamp).toLocaleTimeString()
    : null;

  return (
    <div className="flex gap-2 py-0.5 hover:bg-muted/50 rounded px-1">
      {timestamp && (
        <span className="text-muted-foreground shrink-0 w-16">{timestamp}</span>
      )}
      {entry.level && (
        <Badge
          variant="outline"
          className={cn('h-4 text-[10px] px-1 shrink-0', getLevelBadgeColor(entry.level))}
        >
          {entry.level}
        </Badge>
      )}
      <span className="break-all">{entry.content}</span>
    </div>
  );
}

// ── Add Log Source Dialog ──────────────────────────────────────

interface AddLogSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  componentId: string;
  componentName?: string;
  onSuccess: () => void;
}

function AddLogSourceDialog({
  open,
  onOpenChange,
  componentId,
  componentName,
  onSuccess,
}: AddLogSourceDialogProps) {
  const [sourceType, setSourceType] = useState<'file' | 'event_log'>('file');
  const [name, setName] = useState('');
  const [filePath, setFilePath] = useState('');
  const [logName, setLogName] = useState('Application');
  const [eventSource, setEventSource] = useState('');

  const createSource = useCreateLogSource();

  const handleSubmit = async () => {
    if (!name.trim()) return;

    try {
      await createSource.mutateAsync({
        componentId,
        name: name.trim(),
        source_type: sourceType,
        file_path: sourceType === 'file' ? filePath : undefined,
        log_name: sourceType === 'event_log' ? logName : undefined,
        event_source: sourceType === 'event_log' ? eventSource || undefined : undefined,
      });
      // Reset form
      setName('');
      setFilePath('');
      setLogName('Application');
      setEventSource('');
      onSuccess();
    } catch (error) {
      console.error('Failed to create log source:', error);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add Log Source</DialogTitle>
          <DialogDescription>
            Configure a log source for {componentName || 'this component'}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <Label>Source Type</Label>
            <Select
              value={sourceType}
              onValueChange={(v) => setSourceType(v as 'file' | 'event_log')}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="file">
                  <div className="flex items-center gap-2">
                    <FileText className="h-4 w-4" />
                    <span>Log File</span>
                  </div>
                </SelectItem>
                <SelectItem value="event_log">
                  <div className="flex items-center gap-2">
                    <Monitor className="h-4 w-4" />
                    <span>Windows Event Log</span>
                  </div>
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label htmlFor="name">Name</Label>
            <Input
              id="name"
              placeholder="e.g., Application Log"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          {sourceType === 'file' && (
            <div className="space-y-2">
              <Label htmlFor="filePath">File Path</Label>
              <Input
                id="filePath"
                placeholder="/var/log/myapp/app.log"
                value={filePath}
                onChange={(e) => setFilePath(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                Full path to the log file on the agent machine
              </p>
            </div>
          )}

          {sourceType === 'event_log' && (
            <>
              <div className="space-y-2">
                <Label htmlFor="logName">Event Log Name</Label>
                <Select value={logName} onValueChange={setLogName}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="Application">Application</SelectItem>
                    <SelectItem value="System">System</SelectItem>
                    <SelectItem value="Security">Security</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="eventSource">Event Source (optional)</Label>
                <Input
                  id="eventSource"
                  placeholder="e.g., MyApplication"
                  value={eventSource}
                  onChange={(e) => setEventSource(e.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  Filter events by source name
                </p>
              </div>
            </>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={
              createSource.isPending ||
              !name.trim() ||
              (sourceType === 'file' && !filePath.trim())
            }
          >
            {createSource.isPending ? 'Adding...' : 'Add Source'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default LogSourcesPanel;
