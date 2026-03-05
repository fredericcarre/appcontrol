import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
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
  const isSubscribedRef = useRef(false);
  const lastProcessedMsgRef = useRef(0);

  const wsConnected = useWebSocketStore((s) => s.connected);
  const messages = useWebSocketStore((s) => s.messages);

  // Derive subscription state from refs and props
  const subscriptionState = useMemo(() => {
    if (!open) return 'disconnected';
    if (!wsConnected) return 'disconnected';
    // Note: isSubscribedRef.current is not reactive, but we check wsConnected which is
    return 'connected';
  }, [open, wsConnected]);

  // Subscribe to logs when modal opens
  useEffect(() => {
    if (!open) {
      isSubscribedRef.current = false;
      return;
    }

    // Get WebSocket and send subscribe message
    const ws = getGlobalWebSocket();
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      isSubscribedRef.current = false;
      return;
    }

    const subscribeMsg = {
      type: 'log_subscribe',
      payload: {
        agent_id: agentId || null,
        gateway_id: gatewayId || null,
        min_level: minLevel,
      },
    };

    ws.send(JSON.stringify(subscribeMsg));
    isSubscribedRef.current = true;

    // Cleanup: unsubscribe when modal closes
    return () => {
      const currentWs = getGlobalWebSocket();
      if (currentWs && currentWs.readyState === WebSocket.OPEN) {
        const unsubscribeMsg = {
          type: 'log_unsubscribe',
          payload: {
            agent_id: agentId || null,
            gateway_id: gatewayId || null,
          },
        };
        currentWs.send(JSON.stringify(unsubscribeMsg));
      }
      isSubscribedRef.current = false;
    };
  }, [open, agentId, gatewayId, minLevel]);

  // Re-subscribe when level changes
  useEffect(() => {
    if (!open || !isSubscribedRef.current) return;

    const ws = getGlobalWebSocket();
    if (!ws || ws.readyState !== WebSocket.OPEN) return;

    // Update subscription with new level
    const subscribeMsg = {
      type: 'log_subscribe',
      payload: {
        agent_id: agentId || null,
        gateway_id: gatewayId || null,
        min_level: minLevel,
      },
    };
    ws.send(JSON.stringify(subscribeMsg));
  }, [minLevel, open, agentId, gatewayId]);

  // Process incoming log messages - add entries via callback
  const addLogEntry = useCallback((entry: LogEntry) => {
    setEntries((prev) => {
      const next = [...prev, entry];
      return next.length > 1000 ? next.slice(-1000) : next;
    });
  }, []);

  // Watch for new messages and process them
  useEffect(() => {
    // Process only new messages
    const newMessages = messages.slice(lastProcessedMsgRef.current);
    if (newMessages.length === 0) return;

    lastProcessedMsgRef.current = messages.length;
    const sourceId = agentId || gatewayId;

    for (const msg of newMessages) {
      if (msg.type !== 'LogEntry') continue;

      const payload = msg.payload as unknown as LogEntry;
      if (payload.source_id !== sourceId) continue;

      addLogEntry(payload);
    }
  }, [messages, agentId, gatewayId, addLogEntry]);

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
              {subscriptionState === 'connected' ? (
                <>
                  <Circle className="h-2 w-2 fill-green-500 text-green-500" />
                  <span className="text-green-500">Live</span>
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
