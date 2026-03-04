import { useRef, useEffect, useState, useMemo } from 'react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Search, Trash2, Pause, Play, ChevronDown } from 'lucide-react';

export interface LogEntry {
  source_type: string;
  source_id: string;
  source_name: string;
  level: string;
  target: string;
  message: string;
  timestamp: string;
}

interface LogViewerProps {
  entries: LogEntry[];
  onClear: () => void;
  maxEntries?: number;
}

const LEVEL_COLORS: Record<string, string> = {
  ERROR: 'text-red-500 bg-red-500/10',
  WARN: 'text-amber-500 bg-amber-500/10',
  INFO: 'text-blue-500 bg-blue-500/10',
  DEBUG: 'text-gray-400 bg-gray-500/10',
  TRACE: 'text-gray-500 bg-gray-500/10',
};

const LEVEL_BADGE_CLASSES: Record<string, string> = {
  ERROR: 'bg-red-600 text-white border-red-600',
  WARN: 'bg-amber-600 text-white border-amber-600',
  INFO: 'bg-blue-600 text-white border-blue-600',
  DEBUG: 'bg-gray-600 text-gray-200 border-gray-600',
  TRACE: 'bg-gray-700 text-gray-300 border-gray-700',
};

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts);
    return date.toLocaleTimeString('en-US', {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      fractionalSecondDigits: 3,
    });
  } catch {
    return ts;
  }
}

export function LogViewer({ entries, onClear, maxEntries = 1000 }: LogViewerProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [search, setSearch] = useState('');
  const [userScrolledUp, setUserScrolledUp] = useState(false);

  // Filter entries by search
  const filteredEntries = useMemo(() => {
    if (!search) return entries.slice(-maxEntries);
    const searchLower = search.toLowerCase();
    return entries
      .filter(
        (e) =>
          e.message.toLowerCase().includes(searchLower) ||
          e.target.toLowerCase().includes(searchLower) ||
          e.level.toLowerCase().includes(searchLower)
      )
      .slice(-maxEntries);
  }, [entries, search, maxEntries]);

  // Auto-scroll to bottom when new entries arrive (if enabled)
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filteredEntries, autoScroll]);

  // Handle scroll events to detect user scrolling up
  const handleScroll = () => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;
    setUserScrolledUp(!isAtBottom);
    // Don't auto-enable autoScroll when scrolling to bottom - let user control it
  };

  // Jump to bottom
  const scrollToBottom = () => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
    setUserScrolledUp(false);
    setAutoScroll(true);
  };

  return (
    <div className="flex flex-col h-full bg-gray-950 text-gray-100 font-mono text-sm">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-800 bg-gray-900">
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-500" />
          <Input
            placeholder="Filter logs..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-8 h-8 bg-gray-800 border-gray-700 text-gray-100 placeholder:text-gray-500"
          />
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setAutoScroll(!autoScroll)}
          className={autoScroll ? 'text-green-500' : 'text-gray-500'}
        >
          {autoScroll ? <Pause className="h-4 w-4 mr-1" /> : <Play className="h-4 w-4 mr-1" />}
          {autoScroll ? 'Pause' : 'Resume'}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onClear}
          className="text-gray-400 hover:text-gray-100"
        >
          <Trash2 className="h-4 w-4 mr-1" />
          Clear
        </Button>
        <span className="text-xs text-gray-500">{filteredEntries.length} entries</span>
      </div>

      {/* Log entries */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto p-2 space-y-0.5"
        onScroll={handleScroll}
      >
        {filteredEntries.length === 0 ? (
          <div className="flex items-center justify-center h-full text-gray-500">
            {entries.length === 0 ? 'Waiting for logs...' : 'No matching entries'}
          </div>
        ) : (
          filteredEntries.map((entry, index) => (
            <div
              key={`${entry.timestamp}-${index}`}
              className={`flex gap-2 px-2 py-0.5 rounded hover:bg-gray-800/50 ${LEVEL_COLORS[entry.level] || ''}`}
            >
              <span className="text-gray-500 shrink-0 w-24">
                {formatTimestamp(entry.timestamp)}
              </span>
              <span
                className={`h-5 px-1.5 text-[11px] font-medium shrink-0 w-14 flex items-center justify-center rounded ${LEVEL_BADGE_CLASSES[entry.level] || 'bg-gray-700 text-gray-300'}`}
              >
                {entry.level}
              </span>
              <span className="text-gray-400 shrink-0 truncate max-w-[200px]" title={entry.target}>
                {entry.target}
              </span>
              <span className="flex-1 break-all">{entry.message}</span>
            </div>
          ))
        )}
      </div>

      {/* Scroll to bottom indicator */}
      {userScrolledUp && (
        <div className="absolute bottom-4 right-8 z-10">
          <Button
            size="sm"
            variant="secondary"
            onClick={scrollToBottom}
            className="shadow-lg"
          >
            <ChevronDown className="h-4 w-4 mr-1" />
            Jump to latest
          </Button>
        </div>
      )}
    </div>
  );
}
