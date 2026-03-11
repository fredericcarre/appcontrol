import { useState, useMemo } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Clock,
  Calendar,
  Plus,
  Trash2,
  Server,
  Play,
  Pause,
  CheckCircle,
  Loader2,
  CalendarClock,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useAgents, type Agent } from '@/api/reports';
import {
  useSnapshotSchedules,
  useCreateSchedule,
  useUpdateSchedule,
  useDeleteSchedule,
  type SnapshotSchedule,
  type ScheduleFrequency,
} from '@/api/discovery';

interface ScheduleModalProps {
  open: boolean;
  onClose: () => void;
}

const FREQUENCY_OPTIONS: { value: ScheduleFrequency; label: string; description: string }[] = [
  { value: 'hourly', label: 'Hourly', description: 'Every hour' },
  { value: 'daily', label: 'Daily', description: 'Once per day at midnight' },
  { value: 'weekly', label: 'Weekly', description: 'Every Sunday at midnight' },
  { value: 'monthly', label: 'Monthly', description: 'First day of each month' },
];

export function ScheduleModal({ open, onClose }: ScheduleModalProps) {
  const [mode, setMode] = useState<'list' | 'create'>('list');
  const [newSchedule, setNewSchedule] = useState({
    name: '',
    frequency: 'daily' as ScheduleFrequency,
    retention_days: 30,
    agent_ids: [] as string[],
  });

  const { data: schedules, isLoading: loadingSchedules } = useSnapshotSchedules();
  const { data: agentsData } = useAgents();
  const createSchedule = useCreateSchedule();
  const updateSchedule = useUpdateSchedule();
  const deleteSchedule = useDeleteSchedule();

  const agents: Agent[] = useMemo(() => {
    return Array.isArray(agentsData)
      ? agentsData
      : (agentsData as unknown as { agents?: Agent[] })?.agents || [];
  }, [agentsData]);

  const toggleAgent = (agentId: string) => {
    setNewSchedule((prev) => ({
      ...prev,
      agent_ids: prev.agent_ids.includes(agentId)
        ? prev.agent_ids.filter((id) => id !== agentId)
        : [...prev.agent_ids, agentId],
    }));
  };

  const handleCreate = async () => {
    if (!newSchedule.name || newSchedule.agent_ids.length === 0) return;

    await createSchedule.mutateAsync(newSchedule);
    setNewSchedule({
      name: '',
      frequency: 'daily',
      retention_days: 30,
      agent_ids: [],
    });
    setMode('list');
  };

  const handleToggleEnabled = async (schedule: SnapshotSchedule) => {
    await updateSchedule.mutateAsync({
      id: schedule.id,
      enabled: !schedule.enabled,
    });
  };

  const handleDelete = async (scheduleId: string) => {
    await deleteSchedule.mutateAsync(scheduleId);
  };

  const formatNextRun = (dateStr?: string) => {
    if (!dateStr) return 'Not scheduled';
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = date.getTime() - now.getTime();
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffHours < 1) return 'Less than 1 hour';
    if (diffHours < 24) return `In ${diffHours}h`;
    return `In ${diffDays}d`;
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <CalendarClock className="h-5 w-5" />
            Scheduled Snapshots
          </DialogTitle>
          <DialogDescription>
            Automatically capture discovery snapshots at regular intervals for comparison.
          </DialogDescription>
        </DialogHeader>

        {mode === 'list' ? (
          <>
            <div className="flex-1 overflow-auto">
              {loadingSchedules ? (
                <div className="flex items-center justify-center py-12">
                  <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                </div>
              ) : !schedules || schedules.length === 0 ? (
                <div className="text-center py-12 text-muted-foreground">
                  <CalendarClock className="h-12 w-12 mx-auto mb-3 opacity-50" />
                  <p>No scheduled snapshots yet.</p>
                  <p className="text-sm">Create a schedule to automatically capture discovery data.</p>
                </div>
              ) : (
                <ScrollArea className="h-[400px] pr-4">
                  <div className="space-y-3">
                    {schedules.map((schedule) => (
                      <div
                        key={schedule.id}
                        className={cn(
                          'p-4 rounded-lg border bg-card',
                          !schedule.enabled && 'opacity-60'
                        )}
                      >
                        <div className="flex items-start justify-between">
                          <div className="flex-1">
                            <div className="flex items-center gap-2">
                              <span className="font-medium">{schedule.name}</span>
                              <Badge variant={schedule.enabled ? 'default' : 'secondary'}>
                                {schedule.enabled ? 'Active' : 'Paused'}
                              </Badge>
                            </div>
                            <div className="flex items-center gap-4 mt-2 text-sm text-muted-foreground">
                              <span className="flex items-center gap-1">
                                <Clock className="h-3.5 w-3.5" />
                                {FREQUENCY_OPTIONS.find((f) => f.value === schedule.frequency)?.label}
                              </span>
                              <span className="flex items-center gap-1">
                                <Server className="h-3.5 w-3.5" />
                                {schedule.agent_ids.length} agent{schedule.agent_ids.length !== 1 ? 's' : ''}
                              </span>
                              <span className="flex items-center gap-1">
                                <Calendar className="h-3.5 w-3.5" />
                                Keep {schedule.retention_days}d
                              </span>
                            </div>
                            {schedule.enabled && (
                              <div className="flex items-center gap-4 mt-2 text-xs">
                                <span className="text-emerald-600">
                                  Next: {formatNextRun(schedule.next_run_at)}
                                </span>
                                {schedule.last_run_at && (
                                  <span className="text-muted-foreground">
                                    Last: {new Date(schedule.last_run_at).toLocaleDateString()}
                                  </span>
                                )}
                              </div>
                            )}
                          </div>
                          <div className="flex items-center gap-2">
                            <Button
                              size="icon"
                              variant="ghost"
                              onClick={() => handleToggleEnabled(schedule)}
                              className={cn(
                                'h-8 w-8',
                                schedule.enabled
                                  ? 'text-amber-600 hover:bg-amber-50'
                                  : 'text-emerald-600 hover:bg-emerald-50'
                              )}
                              title={schedule.enabled ? 'Pause' : 'Resume'}
                            >
                              {schedule.enabled ? (
                                <Pause className="h-4 w-4" />
                              ) : (
                                <Play className="h-4 w-4" />
                              )}
                            </Button>
                            <Button
                              size="icon"
                              variant="ghost"
                              onClick={() => handleDelete(schedule.id)}
                              className="h-8 w-8 text-red-600 hover:bg-red-50"
                              title="Delete"
                            >
                              <Trash2 className="h-4 w-4" />
                            </Button>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </ScrollArea>
              )}
            </div>

            <DialogFooter>
              <Button variant="outline" onClick={onClose}>
                Close
              </Button>
              <Button onClick={() => setMode('create')} className="gap-2">
                <Plus className="h-4 w-4" />
                Create Schedule
              </Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <div className="flex-1 overflow-auto space-y-6 py-4">
              {/* Schedule name */}
              <div className="space-y-2">
                <Label htmlFor="schedule-name">Schedule Name</Label>
                <Input
                  id="schedule-name"
                  placeholder="e.g., Daily Production Snapshot"
                  value={newSchedule.name}
                  onChange={(e) => setNewSchedule((prev) => ({ ...prev, name: e.target.value }))}
                />
              </div>

              {/* Frequency */}
              <div className="space-y-2">
                <Label>Frequency</Label>
                <div className="grid grid-cols-2 gap-2">
                  {FREQUENCY_OPTIONS.map((option) => (
                    <button
                      key={option.value}
                      onClick={() => setNewSchedule((prev) => ({ ...prev, frequency: option.value }))}
                      className={cn(
                        'flex flex-col items-start p-3 rounded-lg border text-left transition-all',
                        newSchedule.frequency === option.value
                          ? 'border-primary bg-primary/5 ring-1 ring-primary'
                          : 'border-border hover:bg-accent'
                      )}
                    >
                      <span className={cn(
                        'font-medium text-sm',
                        newSchedule.frequency === option.value && 'text-primary'
                      )}>
                        {option.label}
                      </span>
                      <span className="text-xs text-muted-foreground">{option.description}</span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Retention */}
              <div className="space-y-2">
                <Label htmlFor="retention">Retention (days)</Label>
                <Select
                  value={String(newSchedule.retention_days)}
                  onValueChange={(v) => setNewSchedule((prev) => ({ ...prev, retention_days: Number(v) }))}
                >
                  <SelectTrigger id="retention" className="w-[200px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="7">7 days</SelectItem>
                    <SelectItem value="14">14 days</SelectItem>
                    <SelectItem value="30">30 days</SelectItem>
                    <SelectItem value="60">60 days</SelectItem>
                    <SelectItem value="90">90 days</SelectItem>
                    <SelectItem value="365">1 year</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {/* Agent selection */}
              <div className="space-y-2">
                <Label>Agents to scan</Label>
                <ScrollArea className="h-[150px] border rounded-md p-2">
                  <div className="space-y-1">
                    {agents.map((agent) => {
                      const selected = newSchedule.agent_ids.includes(agent.id);
                      return (
                        <label
                          key={agent.id}
                          className="flex items-center gap-3 p-2 rounded-md hover:bg-accent cursor-pointer"
                        >
                          <input
                            type="checkbox"
                            checked={selected}
                            onChange={() => toggleAgent(agent.id)}
                            className="h-4 w-4 rounded border-gray-300 text-primary focus:ring-primary"
                          />
                          <div className="flex-1 min-w-0">
                            <span className="text-sm font-medium">{agent.hostname || agent.id}</span>
                            {agent.gateway_name && (
                              <span className="text-xs text-muted-foreground ml-2">
                                via {agent.gateway_name}
                              </span>
                            )}
                          </div>
                          <div
                            className={cn(
                              'w-2 h-2 rounded-full',
                              agent.connected ? 'bg-emerald-500' : 'bg-slate-400'
                            )}
                          />
                        </label>
                      );
                    })}
                    {agents.length === 0 && (
                      <p className="text-sm text-muted-foreground text-center py-4">
                        No agents available.
                      </p>
                    )}
                  </div>
                </ScrollArea>
                {newSchedule.agent_ids.length > 0 && (
                  <p className="text-xs text-muted-foreground">
                    {newSchedule.agent_ids.length} agent{newSchedule.agent_ids.length !== 1 ? 's' : ''} selected
                  </p>
                )}
              </div>
            </div>

            <DialogFooter>
              <Button variant="outline" onClick={() => setMode('list')}>
                Cancel
              </Button>
              <Button
                onClick={handleCreate}
                disabled={!newSchedule.name || newSchedule.agent_ids.length === 0 || createSchedule.isPending}
                className="gap-2"
              >
                {createSchedule.isPending ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <CheckCircle className="h-4 w-4" />
                )}
                Create Schedule
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
