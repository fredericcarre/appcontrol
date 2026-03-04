import { useState, useEffect, useCallback } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { LogViewer, type LogEntry } from './LogViewer';
import { FileText, X, Circle, WifiOff } from 'lucide-react';
import { useWebSocketStore } from '@/stores/websocket';
import { getGlobalWebSocket } from '@/hooks/use-websocket';

interface LogViewerModalProps {
  /** Agent ID to stream logs from */
  agentId?: string;
  /** Gateway ID to stream logs from */
  gatewayId?: string;
  /** Display name for the source */
  sourceName: string;
  /** Source type for display */
  sourceType: 'agent' | 'gateway';
  open: boolean;
  onClose: () => void;
}

const LOG_LEVELS = ['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR'] as const;
type LogLevel = (typeof LOG_LEVELS)[number];

export function LogViewerModal({
  agentId,
  gatewayId,
  sourceName,
  sourceType,
  open,
  onClose,
}: LogViewerModalProps) {
  const [minLevel, setMinLevel] = useState<LogLevel>('DEBUG');
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [isSubscribed, setIsSubscribed] = useState(false);

  const wsConnected = useWebSocketStore((s) => s.connected);
  const messages = useWebSocketStore((s) => s.messages);

  // Subscribe to logs when modal opens
  useEffect(() => {
    if (!open) {
      setIsSubscribed(false);
      return;
    }

    // Get WebSocket and send subscribe message
    const ws = getGlobalWebSocket();
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      return;
    }

    const subscribeMsg = {
      type: 'LogSubscribe',
      payload: {
        agent_id: agentId || null,
        gateway_id: gatewayId || null,
        min_level: minLevel,
      },
    };

    ws.send(JSON.stringify(subscribeMsg));
    setIsSubscribed(true);

    // Cleanup: unsubscribe when modal closes
    return () => {
      const currentWs = getGlobalWebSocket();
      if (currentWs && currentWs.readyState === WebSocket.OPEN) {
        const unsubscribeMsg = {
          type: 'LogUnsubscribe',
          payload: {
            agent_id: agentId || null,
            gateway_id: gatewayId || null,
          },
        };
        currentWs.send(JSON.stringify(unsubscribeMsg));
      }
      setIsSubscribed(false);
    };
  }, [open, agentId, gatewayId, minLevel]);

  // Re-subscribe when level changes
  useEffect(() => {
    if (!open || !isSubscribed) return;

    const ws = getGlobalWebSocket();
    if (!ws || ws.readyState !== WebSocket.OPEN) return;

    // Update subscription with new level
    const subscribeMsg = {
      type: 'LogSubscribe',
      payload: {
        agent_id: agentId || null,
        gateway_id: gatewayId || null,
        min_level: minLevel,
      },
    };
    ws.send(JSON.stringify(subscribeMsg));
  }, [minLevel]);

  // Process incoming log messages
  useEffect(() => {
    // Get the latest message
    const lastMsg = messages[messages.length - 1];
    if (!lastMsg || lastMsg.type !== 'LogEntry') return;

    const payload = lastMsg.payload as unknown as LogEntry;

    // Check if this message is for our source
    const sourceId = agentId || gatewayId;
    if (payload.source_id !== sourceId) return;

    // Add to entries (keep last 1000)
    setEntries((prev) => {
      const next = [...prev, payload];
      if (next.length > 1000) {
        return next.slice(-1000);
      }
      return next;
    });
  }, [messages, agentId, gatewayId]);

  const handleClear = useCallback(() => {
    setEntries([]);
  }, []);

  const handleClose = () => {
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent className="max-w-5xl h-[700px] flex flex-col p-0 gap-0">
        <DialogHeader className="px-4 py-3 border-b flex flex-row items-center justify-between space-y-0">
          <div className="flex items-center gap-3">
            <FileText className="h-5 w-5 text-muted-foreground" />
            <DialogTitle className="text-lg">
              Logs: {sourceName}
            </DialogTitle>
            <Badge variant="outline" className="text-xs">
              {sourceType}
            </Badge>
          </div>
          <div className="flex items-center gap-3">
            {/* Connection status */}
            <div className="flex items-center gap-1.5 text-sm">
              {wsConnected && isSubscribed ? (
                <>
                  <Circle className="h-2 w-2 fill-green-500 text-green-500" />
                  <span className="text-green-500">Live</span>
                </>
              ) : wsConnected ? (
                <>
                  <Circle className="h-2 w-2 fill-yellow-500 text-yellow-500" />
                  <span className="text-yellow-500">Connecting...</span>
                </>
              ) : (
                <>
                  <WifiOff className="h-3 w-3 text-red-500" />
                  <span className="text-red-500">Disconnected</span>
                </>
              )}
            </div>

            {/* Level filter */}
            <Select value={minLevel} onValueChange={(v) => setMinLevel(v as LogLevel)}>
              <SelectTrigger className="w-[110px] h-8">
                <SelectValue placeholder="Level" />
              </SelectTrigger>
              <SelectContent>
                {LOG_LEVELS.map((level) => (
                  <SelectItem key={level} value={level}>
                    {level}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Button variant="ghost" size="icon" className="h-8 w-8" onClick={handleClose}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </DialogHeader>
        <div className="flex-1 min-h-0 relative">
          <LogViewer entries={entries} onClear={handleClear} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
