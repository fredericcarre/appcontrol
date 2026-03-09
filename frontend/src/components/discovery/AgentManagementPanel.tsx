import { useState, useMemo } from 'react';
import { X, AlertTriangle, Trash2, Server, Clock, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useAgents, useBulkDeleteAgents, type Agent } from '@/api/reports';
import { cn } from '@/lib/utils';

interface AgentManagementPanelProps {
  open: boolean;
  onClose: () => void;
}

export function AgentManagementPanel({ open, onClose }: AgentManagementPanelProps) {
  const { data: agents, isLoading } = useAgents();
  const bulkDelete = useBulkDeleteAgents();
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [thresholdDays, setThresholdDays] = useState(7);

  // Filter stale agents (not seen for more than threshold days)
  const staleAgents = useMemo(() => {
    if (!agents) return [];
    const threshold = Date.now() - thresholdDays * 24 * 60 * 60 * 1000;
    return agents.filter((a) => {
      if (!a.last_heartbeat_at) return true; // Never connected
      const lastSeen = new Date(a.last_heartbeat_at).getTime();
      return lastSeen < threshold;
    });
  }, [agents, thresholdDays]);

  const toggleAgent = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => {
    setSelectedIds(new Set(staleAgents.map((a) => a.id)));
  };

  const selectNone = () => {
    setSelectedIds(new Set());
  };

  const handleDelete = async () => {
    if (selectedIds.size === 0) return;
    try {
      await bulkDelete.mutateAsync([...selectedIds]);
      setSelectedIds(new Set());
    } catch (err) {
      console.error('Failed to delete agents:', err);
    }
  };

  const formatLastSeen = (lastHeartbeat: string | null) => {
    if (!lastHeartbeat) return 'Never';
    const date = new Date(lastHeartbeat);
    const days = Math.floor((Date.now() - date.getTime()) / (1000 * 60 * 60 * 24));
    if (days === 0) return 'Today';
    if (days === 1) return 'Yesterday';
    return `${days} days ago`;
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-end">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative z-50 h-full w-full max-w-md bg-background shadow-xl flex flex-col animate-in slide-in-from-right-full duration-300">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <div className="flex items-center gap-2">
            <AlertTriangle className="h-5 w-5 text-amber-500" />
            <h2 className="font-semibold">Manage Stale Agents</h2>
          </div>
          <button onClick={onClose} className="p-1 rounded hover:bg-accent">
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Filter */}
        <div className="px-4 py-3 border-b border-border">
          <label className="text-sm text-muted-foreground">
            Show agents not seen for more than:
          </label>
          <div className="flex items-center gap-2 mt-1">
            <select
              value={thresholdDays}
              onChange={(e) => setThresholdDays(Number(e.target.value))}
              className="flex h-9 rounded-md border border-input bg-background px-3 py-1 text-sm"
            >
              <option value={1}>1 day</option>
              <option value={3}>3 days</option>
              <option value={7}>7 days</option>
              <option value={14}>14 days</option>
              <option value={30}>30 days</option>
            </select>
            <Badge variant="secondary">{staleAgents.length} stale</Badge>
          </div>
        </div>

        {/* Selection controls */}
        <div className="flex items-center justify-between px-4 py-2 border-b border-border">
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={selectAll} className="text-xs">
              Select All
            </Button>
            <Button variant="ghost" size="sm" onClick={selectNone} className="text-xs">
              Select None
            </Button>
          </div>
          <Badge variant={selectedIds.size > 0 ? 'default' : 'secondary'}>
            {selectedIds.size} selected
          </Badge>
        </div>

        {/* Agent list */}
        <ScrollArea className="flex-1">
          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
          ) : staleAgents.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
              <Server className="h-10 w-10 mb-2" />
              <p className="text-sm">No stale agents found</p>
              <p className="text-xs">All agents are active</p>
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {staleAgents.map((agent) => {
                const isSelected = selectedIds.has(agent.id);
                return (
                  <label
                    key={agent.id}
                    className={cn(
                      'flex items-center gap-3 p-3 rounded-lg cursor-pointer transition-colors',
                      isSelected ? 'bg-destructive/10 border border-destructive/30' : 'hover:bg-accent'
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleAgent(agent.id)}
                      className="h-4 w-4 rounded border-gray-300 text-destructive focus:ring-destructive"
                    />
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <Server className="h-4 w-4 text-slate-400" />
                        <span className="font-medium text-sm">{agent.hostname}</span>
                        {!agent.is_active && (
                          <Badge variant="outline" className="text-[10px]">Inactive</Badge>
                        )}
                      </div>
                      <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground">
                        {agent.gateway_name && (
                          <span>{agent.gateway_name}</span>
                        )}
                        <span className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {formatLastSeen(agent.last_heartbeat_at)}
                        </span>
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          )}
        </ScrollArea>

        {/* Footer */}
        <div className="px-4 py-3 border-t border-border">
          <Button
            variant="destructive"
            className="w-full gap-2"
            disabled={selectedIds.size === 0 || bulkDelete.isPending}
            onClick={handleDelete}
          >
            {bulkDelete.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Trash2 className="h-4 w-4" />
            )}
            Delete {selectedIds.size} Agent{selectedIds.size !== 1 ? 's' : ''}
          </Button>
          {selectedIds.size > 0 && (
            <p className="text-xs text-muted-foreground text-center mt-2">
              Components will remain but will be marked as unreachable
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
