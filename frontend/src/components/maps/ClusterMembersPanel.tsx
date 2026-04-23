import { useMemo, useState } from 'react';
import { Play, Square, Plus, Trash2, Server } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  useClusterMembers,
  useCreateClusterMember,
  useUpdateClusterMember,
  useDeleteClusterMember,
  useBatchStartMembers,
  useBatchStopMembers,
  type ClusterMember,
} from '@/api/clusterMembers';
import { useAgents } from '@/api/reports';

interface ClusterMembersPanelProps {
  componentId: string;
  canOperate?: boolean;
  canEdit?: boolean;
}

function stateDotClass(state?: string): string {
  switch ((state || '').toUpperCase()) {
    case 'RUNNING':
      return 'bg-emerald-500';
    case 'DEGRADED':
      return 'bg-amber-500';
    case 'FAILED':
      return 'bg-red-500';
    case 'STOPPED':
      return 'bg-gray-400';
    case 'STARTING':
    case 'STOPPING':
      return 'bg-blue-500';
    case 'UNREACHABLE':
      return 'bg-slate-600';
    default:
      return 'bg-gray-400';
  }
}

export function ClusterMembersPanel({
  componentId,
  canOperate = false,
  canEdit = false,
}: ClusterMembersPanelProps) {
  const { data: members = [], isLoading } = useClusterMembers(componentId);
  const { data: agents = [] } = useAgents();
  const createMember = useCreateClusterMember(componentId);
  const updateMember = useUpdateClusterMember(componentId);
  const deleteMember = useDeleteClusterMember(componentId);
  const batchStart = useBatchStartMembers(componentId);
  const batchStop = useBatchStopMembers(componentId);

  const [editing, setEditing] = useState<ClusterMember | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  const summary = useMemo(() => {
    const total = members.length;
    const running = members.filter((m) => m.current_state === 'RUNNING').length;
    const degraded = members.filter((m) => m.current_state === 'DEGRADED').length;
    const failed = members.filter((m) => m.current_state === 'FAILED').length;
    const stopped = members.filter((m) => m.current_state === 'STOPPED').length;
    return { total, running, degraded, failed, stopped };
  }, [members]);

  const toggleSelect = (id: string) => {
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    setSelectedIds(next);
  };

  const selectedArray = useMemo(() => Array.from(selectedIds), [selectedIds]);

  if (isLoading) {
    return <div className="p-4 text-sm text-muted-foreground">Loading members…</div>;
  }

  return (
    <div className="flex flex-col gap-3 p-4">
      {/* Aggregate summary */}
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <Badge variant="outline" className="gap-1">
          <Server className="h-3 w-3" />
          {summary.total} member{summary.total !== 1 ? 's' : ''}
        </Badge>
        <Badge className="bg-emerald-500/15 text-emerald-700 dark:text-emerald-400">
          {summary.running} RUNNING
        </Badge>
        {summary.degraded > 0 && (
          <Badge className="bg-amber-500/15 text-amber-700 dark:text-amber-400">
            {summary.degraded} DEGRADED
          </Badge>
        )}
        {summary.failed > 0 && (
          <Badge className="bg-red-500/15 text-red-700 dark:text-red-400">
            {summary.failed} FAILED
          </Badge>
        )}
        {summary.stopped > 0 && (
          <Badge className="bg-gray-500/15 text-gray-700 dark:text-gray-400">
            {summary.stopped} STOPPED
          </Badge>
        )}
      </div>

      {/* Batch actions */}
      <div className="flex flex-wrap gap-2">
        {canOperate && (
          <>
            <Button
              size="sm"
              variant="outline"
              disabled={batchStart.isPending}
              onClick={() =>
                batchStart.mutate({
                  member_ids: selectedArray.length > 0 ? selectedArray : undefined,
                })
              }
            >
              <Play className="mr-1 h-3.5 w-3.5" />
              Start {selectedArray.length > 0 ? `(${selectedArray.length})` : 'all'}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={batchStop.isPending}
              onClick={() =>
                batchStop.mutate({
                  member_ids: selectedArray.length > 0 ? selectedArray : undefined,
                })
              }
            >
              <Square className="mr-1 h-3.5 w-3.5" />
              Stop {selectedArray.length > 0 ? `(${selectedArray.length})` : 'all'}
            </Button>
          </>
        )}
        {canEdit && (
          <Button size="sm" onClick={() => setShowAdd(true)}>
            <Plus className="mr-1 h-3.5 w-3.5" />
            Add member
          </Button>
        )}
      </div>

      {/* Members table */}
      <ScrollArea className="max-h-[400px] rounded-md border">
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-muted/50">
            <tr className="text-left">
              <th className="w-8 p-2"></th>
              <th className="p-2">Hostname</th>
              <th className="p-2">State</th>
              <th className="p-2">Agent</th>
              <th className="p-2 text-right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {members.length === 0 ? (
              <tr>
                <td colSpan={5} className="p-4 text-center text-muted-foreground">
                  No members yet. Add one to start monitoring.
                </td>
              </tr>
            ) : (
              members.map((m) => {
                const agent = agents.find((a) => a.id === m.agent_id);
                return (
                  <tr key={m.id} className="border-t hover:bg-muted/30">
                    <td className="p-2">
                      <input
                        type="checkbox"
                        aria-label={`Select ${m.hostname}`}
                        checked={selectedIds.has(m.id)}
                        onChange={() => toggleSelect(m.id)}
                      />
                    </td>
                    <td className="p-2 font-mono text-xs">
                      <div className="flex items-center gap-2">
                        {m.hostname}
                        {!m.is_enabled && (
                          <Badge variant="outline" className="text-xs">
                            disabled
                          </Badge>
                        )}
                      </div>
                    </td>
                    <td className="p-2">
                      <div className="flex items-center gap-1.5">
                        <span
                          className={`inline-block h-2 w-2 rounded-full ${stateDotClass(
                            m.current_state,
                          )}`}
                        />
                        <span className="text-xs">{m.current_state || 'UNKNOWN'}</span>
                      </div>
                    </td>
                    <td className="p-2 text-xs text-muted-foreground">
                      {agent?.hostname || m.agent_id.slice(0, 8)}
                    </td>
                    <td className="p-2 text-right">
                      {canEdit && (
                        <>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => setEditing(m)}
                          >
                            Edit
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            className="text-destructive"
                            onClick={() => {
                              if (confirm(`Remove ${m.hostname}?`)) {
                                deleteMember.mutate(m.id);
                              }
                            }}
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </Button>
                        </>
                      )}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </ScrollArea>

      {/* Add/Edit dialog */}
      {(showAdd || editing) && (
        <MemberEditorDialog
          componentId={componentId}
          member={editing}
          agents={agents.map((a) => ({ id: a.id, hostname: a.hostname }))}
          onClose={() => {
            setShowAdd(false);
            setEditing(null);
          }}
          onCreate={(payload) => createMember.mutateAsync(payload)}
          onUpdate={(id, payload) => updateMember.mutateAsync({ id, payload })}
          isSaving={createMember.isPending || updateMember.isPending}
        />
      )}
    </div>
  );
}

interface MemberEditorDialogProps {
  componentId: string;
  member: ClusterMember | null;
  agents: Array<{ id: string; hostname: string }>;
  onClose: () => void;
  onCreate: (payload: {
    hostname: string;
    agent_id: string;
    install_path?: string | null;
    check_cmd_override?: string | null;
    start_cmd_override?: string | null;
    stop_cmd_override?: string | null;
    is_enabled?: boolean;
  }) => Promise<unknown>;
  onUpdate: (
    id: string,
    payload: {
      hostname?: string;
      agent_id?: string;
      install_path?: string | null;
      check_cmd_override?: string | null;
      start_cmd_override?: string | null;
      stop_cmd_override?: string | null;
      is_enabled?: boolean;
    },
  ) => Promise<unknown>;
  isSaving: boolean;
}

function MemberEditorDialog({
  member,
  agents,
  onClose,
  onCreate,
  onUpdate,
  isSaving,
}: MemberEditorDialogProps) {
  const [hostname, setHostname] = useState(member?.hostname ?? '');
  const [agentId, setAgentId] = useState(member?.agent_id ?? (agents[0]?.id ?? ''));
  const [installPath, setInstallPath] = useState(member?.install_path ?? '');
  const [checkOverride, setCheckOverride] = useState(member?.check_cmd_override ?? '');
  const [startOverride, setStartOverride] = useState(member?.start_cmd_override ?? '');
  const [stopOverride, setStopOverride] = useState(member?.stop_cmd_override ?? '');
  const [enabled, setEnabled] = useState(member?.is_enabled ?? true);

  const isNew = !member;

  const handleSave = async () => {
    const payload = {
      hostname,
      agent_id: agentId,
      install_path: installPath || null,
      check_cmd_override: checkOverride || null,
      start_cmd_override: startOverride || null,
      stop_cmd_override: stopOverride || null,
      is_enabled: enabled,
    };
    if (isNew) {
      await onCreate(payload);
    } else {
      await onUpdate(member!.id, payload);
    }
    onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{isNew ? 'Add Cluster Member' : `Edit ${member!.hostname}`}</DialogTitle>
          <DialogDescription>
            Each member is monitored and operated independently. Leave overrides
            blank to inherit the component&apos;s default commands.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-2">
          <div className="space-y-1">
            <Label htmlFor="member-hostname">Hostname *</Label>
            <Input
              id="member-hostname"
              value={hostname}
              onChange={(e) => setHostname(e.target.value)}
              placeholder="srv-jboss-042.prod"
              className="font-mono text-sm"
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="member-agent">Agent *</Label>
            <select
              id="member-agent"
              className="w-full rounded-md border bg-background p-2 text-sm"
              value={agentId}
              onChange={(e) => setAgentId(e.target.value)}
            >
              <option value="">Select an agent…</option>
              {agents.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.hostname}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-1">
            <Label htmlFor="member-install">Install path (optional)</Label>
            <Input
              id="member-install"
              value={installPath}
              onChange={(e) => setInstallPath(e.target.value)}
              placeholder="/opt/jboss"
              className="font-mono text-sm"
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="member-check">Check override (optional)</Label>
            <Input
              id="member-check"
              value={checkOverride}
              onChange={(e) => setCheckOverride(e.target.value)}
              placeholder="Leave empty to inherit"
              className="font-mono text-sm"
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="member-start">Start override (optional)</Label>
            <Input
              id="member-start"
              value={startOverride}
              onChange={(e) => setStartOverride(e.target.value)}
              className="font-mono text-sm"
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="member-stop">Stop override (optional)</Label>
            <Input
              id="member-stop"
              value={stopOverride}
              onChange={(e) => setStopOverride(e.target.value)}
              className="font-mono text-sm"
            />
          </div>

          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
            />
            Enabled (counts toward aggregation)
          </label>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!hostname || !agentId || isSaving}>
            {isSaving ? 'Saving…' : isNew ? 'Add member' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
