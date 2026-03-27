import { useState } from 'react';
import {
  Schedule,
  useAppSchedules,
  useComponentSchedules,
  useToggleSchedule,
  useDeleteSchedule,
  useRunScheduleNow,
  getOperationColor,
  getStatusColor,
} from '@/api/schedules';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ConfirmDialog } from '@/components/ui/confirm-dialog';
import {
  Play,
  Square,
  RotateCcw,
  Clock,
  MoreVertical,
  Pencil,
  Trash2,
  PlayCircle,
  Calendar,
  Plus,
  AlertCircle,
  CheckCircle2,
  XCircle,
  Timer,
} from 'lucide-react';
import { ScheduleDialog } from './ScheduleDialog';

interface ScheduleListProps {
  appId?: string;
  componentId?: string;
  canOperate?: boolean;
}

function OperationIcon({ operation }: { operation: Schedule['operation'] }) {
  switch (operation) {
    case 'start':
      return <Play className="h-3.5 w-3.5" />;
    case 'stop':
      return <Square className="h-3.5 w-3.5" />;
    case 'restart':
      return <RotateCcw className="h-3.5 w-3.5" />;
  }
}

function StatusIcon({ status }: { status: Schedule['last_run_status'] }) {
  switch (status) {
    case 'success':
      return <CheckCircle2 className="h-3.5 w-3.5 text-green-600" />;
    case 'failed':
      return <XCircle className="h-3.5 w-3.5 text-red-600" />;
    case 'skipped':
      return <AlertCircle className="h-3.5 w-3.5 text-yellow-600" />;
    default:
      return <Timer className="h-3.5 w-3.5 text-gray-400" />;
  }
}

export function ScheduleList({ appId, componentId, canOperate = false }: ScheduleListProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingSchedule, setEditingSchedule] = useState<Schedule | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Schedule | null>(null);

  // Use appropriate hook based on target type
  const { data: schedules, isLoading } = appId
    ? useAppSchedules(appId)
    : useComponentSchedules(componentId || '');

  const toggleSchedule = useToggleSchedule();
  const deleteSchedule = useDeleteSchedule();
  const runNow = useRunScheduleNow();

  const handleToggle = (schedule: Schedule) => {
    toggleSchedule.mutate({
      id: schedule.id,
      appId,
      componentId,
    });
  };

  const handleRunNow = (schedule: Schedule) => {
    runNow.mutate({
      id: schedule.id,
      appId,
      componentId,
    });
  };

  const handleEdit = (schedule: Schedule) => {
    setEditingSchedule(schedule);
    setDialogOpen(true);
  };

  const handleDelete = (schedule: Schedule) => {
    setConfirmDelete(schedule);
  };

  const confirmDeleteSchedule = () => {
    if (confirmDelete) {
      deleteSchedule.mutate({
        id: confirmDelete.id,
        appId,
        componentId,
      });
      setConfirmDelete(null);
    }
  };

  const handleCreateNew = () => {
    setEditingSchedule(null);
    setDialogOpen(true);
  };

  const handleDialogClose = () => {
    setDialogOpen(false);
    setEditingSchedule(null);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="animate-spin h-6 w-6 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  const hasSchedules = schedules && schedules.length > 0;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Calendar className="h-4 w-4 text-muted-foreground" />
          <h3 className="text-sm font-semibold">Scheduled Operations</h3>
          {hasSchedules && (
            <Badge variant="secondary" className="text-xs">
              {schedules.length}
            </Badge>
          )}
        </div>
        {canOperate && (
          <Button variant="outline" size="sm" onClick={handleCreateNew}>
            <Plus className="h-3.5 w-3.5 mr-1" />
            New Schedule
          </Button>
        )}
      </div>

      {/* Empty state */}
      {!hasSchedules && (
        <div className="flex flex-col items-center justify-center py-12 text-center">
          <Clock className="h-12 w-12 text-muted-foreground/30 mb-4" />
          <p className="text-sm text-muted-foreground mb-1">No schedules configured</p>
          <p className="text-xs text-muted-foreground mb-4">
            Schedule automated start, stop, or restart operations
          </p>
          {canOperate && (
            <Button variant="outline" size="sm" onClick={handleCreateNew}>
              <Plus className="h-3.5 w-3.5 mr-1" />
              Create Schedule
            </Button>
          )}
        </div>
      )}

      {/* Schedule list */}
      {hasSchedules && (
        <div className="space-y-2">
          {schedules.map((schedule) => (
            <div
              key={schedule.id}
              className={`border rounded-lg p-3 transition-colors ${
                schedule.is_enabled
                  ? 'bg-card hover:bg-accent/50'
                  : 'bg-muted/50 opacity-60'
              }`}
            >
              <div className="flex items-start justify-between gap-3">
                {/* Left: Schedule info */}
                <div className="flex-1 min-w-0 space-y-1.5">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-sm truncate">{schedule.name}</span>
                    <Badge className={`text-[10px] h-5 ${getOperationColor(schedule.operation)}`}>
                      <OperationIcon operation={schedule.operation} />
                      <span className="ml-1 capitalize">{schedule.operation}</span>
                    </Badge>
                  </div>

                  {/* Cron description */}
                  <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                    <Clock className="h-3 w-3" />
                    <span>{schedule.cron_human}</span>
                    <span className="text-muted-foreground/50">({schedule.timezone})</span>
                  </div>

                  {/* Next run */}
                  {schedule.is_enabled && schedule.next_run_relative && (
                    <div className="flex items-center gap-1.5 text-xs">
                      <Timer className="h-3 w-3 text-blue-500" />
                      <span className="text-blue-600 dark:text-blue-400">
                        Next: {schedule.next_run_relative}
                      </span>
                    </div>
                  )}

                  {/* Last run status */}
                  {schedule.last_run_at && (
                    <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <StatusIcon status={schedule.last_run_status} />
                      <span>
                        Last run: {new Date(schedule.last_run_at).toLocaleString()}
                      </span>
                      {schedule.last_run_status && (
                        <Badge
                          variant="outline"
                          className={`text-[10px] h-4 ${getStatusColor(schedule.last_run_status)}`}
                        >
                          {schedule.last_run_status}
                        </Badge>
                      )}
                    </div>
                  )}

                  {/* Error message if failed */}
                  {schedule.last_run_status === 'failed' && schedule.last_run_message && (
                    <div className="text-xs text-red-600 bg-red-50 dark:bg-red-900/20 px-2 py-1 rounded">
                      {schedule.last_run_message}
                    </div>
                  )}
                </div>

                {/* Right: Actions */}
                <div className="flex items-center gap-2">
                  {/* Enable/Disable toggle */}
                  {canOperate && (
                    <Switch
                      checked={schedule.is_enabled}
                      onCheckedChange={() => handleToggle(schedule)}
                      disabled={toggleSchedule.isPending}
                    />
                  )}

                  {/* More actions */}
                  {canOperate && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="icon" className="h-8 w-8">
                          <MoreVertical className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem
                          onClick={() => handleRunNow(schedule)}
                          disabled={!schedule.is_enabled || runNow.isPending}
                        >
                          <PlayCircle className="h-4 w-4 mr-2" />
                          Run Now
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleEdit(schedule)}>
                          <Pencil className="h-4 w-4 mr-2" />
                          Edit
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                          onClick={() => handleDelete(schedule)}
                          className="text-red-600 focus:text-red-600"
                        >
                          <Trash2 className="h-4 w-4 mr-2" />
                          Delete
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Create/Edit Dialog */}
      <ScheduleDialog
        open={dialogOpen}
        onClose={handleDialogClose}
        schedule={editingSchedule}
        appId={appId}
        componentId={componentId}
      />

      {/* Delete Confirmation */}
      <ConfirmDialog
        open={confirmDelete !== null}
        onOpenChange={(open) => !open && setConfirmDelete(null)}
        title="Delete Schedule"
        description={`Are you sure you want to delete the schedule "${confirmDelete?.name}"? This action cannot be undone.`}
        confirmLabel="Delete"
        variant="destructive"
        onConfirm={confirmDeleteSchedule}
      />
    </div>
  );
}
