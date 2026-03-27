import { useState, useEffect, useMemo } from 'react';
import {
  Schedule,
  useSchedulePresets,
  useCreateAppSchedule,
  useCreateComponentSchedule,
  useUpdateSchedule,
} from '@/api/schedules';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { ScrollArea } from '@/components/ui/scroll-area';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import { Play, Square, RotateCcw, Clock, AlertCircle, CheckCircle2 } from 'lucide-react';

interface ScheduleDialogProps {
  open: boolean;
  onClose: () => void;
  schedule?: Schedule | null;
  appId?: string;
  componentId?: string;
}

const COMMON_TIMEZONES = [
  'UTC',
  'Europe/Paris',
  'Europe/London',
  'America/New_York',
  'America/Los_Angeles',
  'America/Chicago',
  'Asia/Tokyo',
  'Asia/Shanghai',
  'Asia/Singapore',
  'Australia/Sydney',
];

type Operation = 'start' | 'stop' | 'restart';

export function ScheduleDialog({
  open,
  onClose,
  schedule,
  appId,
  componentId,
}: ScheduleDialogProps) {
  const isEditing = !!schedule;

  // Form state
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [operation, setOperation] = useState<Operation>('start');
  const [scheduleType, setScheduleType] = useState<'preset' | 'custom'>('preset');
  const [selectedPreset, setSelectedPreset] = useState<string>('');
  const [cronExpression, setCronExpression] = useState('');
  const [timezone, setTimezone] = useState('Europe/Paris');
  const [cronError, setCronError] = useState<string | null>(null);

  // Fetch presets
  const { data: presets } = useSchedulePresets();

  // Mutations
  const createAppSchedule = useCreateAppSchedule();
  const createComponentSchedule = useCreateComponentSchedule();
  const updateSchedule = useUpdateSchedule();

  const isPending =
    createAppSchedule.isPending ||
    createComponentSchedule.isPending ||
    updateSchedule.isPending;

  // Reset form when dialog opens/closes or schedule changes
  // This is a valid pattern for form reset when dialog opens
  /* eslint-disable react-hooks/set-state-in-effect */
  useEffect(() => {
    if (open) {
      if (schedule) {
        setName(schedule.name);
        setDescription(schedule.description || '');
        setOperation(schedule.operation);
        setTimezone(schedule.timezone);
        // Check if the cron matches a preset
        const matchingPreset = presets?.find((p) => p.cron === schedule.cron_expression);
        if (matchingPreset) {
          setScheduleType('preset');
          setSelectedPreset(matchingPreset.id);
          setCronExpression('');
        } else {
          setScheduleType('custom');
          setSelectedPreset('');
          setCronExpression(schedule.cron_expression);
        }
      } else {
        // Reset to defaults for new schedule
        setName('');
        setDescription('');
        setOperation('start');
        setScheduleType('preset');
        setSelectedPreset(presets?.[0]?.id || '');
        setCronExpression('');
        setTimezone('Europe/Paris');
      }
      setCronError(null);
    }
  }, [open, schedule, presets]);
  /* eslint-enable react-hooks/set-state-in-effect */

  // Validate cron expression (basic validation)
  const validateCron = (cron: string): boolean => {
    if (!cron.trim()) {
      setCronError('Cron expression is required');
      return false;
    }
    // Basic validation: should have 5 parts
    const parts = cron.trim().split(/\s+/);
    if (parts.length !== 5) {
      setCronError('Cron expression must have 5 parts (minute hour day month weekday)');
      return false;
    }
    setCronError(null);
    return true;
  };

  // Get the effective cron expression
  const effectiveCron = useMemo(() => {
    if (scheduleType === 'preset') {
      return presets?.find((p) => p.id === selectedPreset)?.cron || '';
    }
    return cronExpression;
  }, [scheduleType, selectedPreset, cronExpression, presets]);

  // Get human-readable description for the selected preset
  const selectedPresetData = useMemo(() => {
    return presets?.find((p) => p.id === selectedPreset);
  }, [presets, selectedPreset]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (scheduleType === 'custom' && !validateCron(cronExpression)) {
      return;
    }

    const payload = {
      name,
      description: description || undefined,
      operation,
      timezone,
      ...(scheduleType === 'preset'
        ? { preset: selectedPreset }
        : { cron_expression: cronExpression }),
    };

    try {
      if (isEditing && schedule) {
        await updateSchedule.mutateAsync({
          id: schedule.id,
          appId,
          componentId,
          ...payload,
        });
      } else if (appId) {
        await createAppSchedule.mutateAsync({
          appId,
          ...payload,
        });
      } else if (componentId) {
        await createComponentSchedule.mutateAsync({
          componentId,
          ...payload,
        });
      }
      onClose();
    } catch (error) {
      console.error('Failed to save schedule:', error);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(isOpen) => !isOpen && onClose()}>
      <DialogContent className="max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{isEditing ? 'Edit Schedule' : 'Create Schedule'}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col flex-1 min-h-0">
          <ScrollArea className="flex-1 pr-4 -mr-4">
            <div className="space-y-5 pb-2">
          {/* Name */}
          <div className="space-y-2">
            <Label htmlFor="name">Name</Label>
            <Input
              id="name"
              placeholder="e.g., Daily morning start"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
            />
          </div>

          {/* Description */}
          <div className="space-y-2">
            <Label htmlFor="description">Description (optional)</Label>
            <Textarea
              id="description"
              placeholder="Describe the purpose of this schedule..."
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
            />
          </div>

          {/* Operation */}
          <div className="space-y-2">
            <Label>Operation</Label>
            <RadioGroup
              value={operation}
              onValueChange={(v) => setOperation(v as Operation)}
              className="flex gap-4"
            >
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="start" id="op-start" />
                <Label
                  htmlFor="op-start"
                  className="flex items-center gap-1.5 cursor-pointer"
                >
                  <Play className="h-4 w-4 text-green-600" />
                  Start
                </Label>
              </div>
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="stop" id="op-stop" />
                <Label
                  htmlFor="op-stop"
                  className="flex items-center gap-1.5 cursor-pointer"
                >
                  <Square className="h-4 w-4 text-red-600" />
                  Stop
                </Label>
              </div>
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="restart" id="op-restart" />
                <Label
                  htmlFor="op-restart"
                  className="flex items-center gap-1.5 cursor-pointer"
                >
                  <RotateCcw className="h-4 w-4 text-blue-600" />
                  Restart
                </Label>
              </div>
            </RadioGroup>
          </div>

          {/* Schedule Type Tabs */}
          <div className="space-y-2">
            <Label>Schedule</Label>
            <Tabs value={scheduleType} onValueChange={(v) => setScheduleType(v as 'preset' | 'custom')}>
              <TabsList className="w-full">
                <TabsTrigger value="preset" className="flex-1">
                  Presets
                </TabsTrigger>
                <TabsTrigger value="custom" className="flex-1">
                  Custom Cron
                </TabsTrigger>
              </TabsList>

              <TabsContent value="preset" className="mt-3">
                {presets && presets.length > 0 ? (
                  <div className="grid grid-cols-2 gap-2">
                    {presets.map((preset) => (
                      <button
                        key={preset.id}
                        type="button"
                        onClick={() => setSelectedPreset(preset.id)}
                        className={`p-3 border rounded-lg text-left transition-colors ${
                          selectedPreset === preset.id
                            ? 'border-primary bg-primary/5'
                            : 'border-border hover:bg-accent/50'
                        }`}
                      >
                        <div className="font-medium text-sm">{preset.label}</div>
                        <div className="text-xs text-muted-foreground mt-0.5">
                          {preset.description}
                        </div>
                        <code className="text-[10px] text-muted-foreground/70 mt-1 block">
                          {preset.cron}
                        </code>
                      </button>
                    ))}
                  </div>
                ) : (
                  <div className="text-center py-4 text-sm text-muted-foreground">
                    Loading presets...
                  </div>
                )}
              </TabsContent>

              <TabsContent value="custom" className="mt-3 space-y-3">
                <div className="space-y-2">
                  <Input
                    placeholder="0 7 * * 1-5 (7am on weekdays)"
                    value={cronExpression}
                    onChange={(e) => {
                      setCronExpression(e.target.value);
                      setCronError(null);
                    }}
                    className={cronError ? 'border-red-500' : ''}
                  />
                  {cronError && (
                    <div className="flex items-center gap-1.5 text-xs text-red-600">
                      <AlertCircle className="h-3.5 w-3.5" />
                      {cronError}
                    </div>
                  )}
                </div>
                <div className="text-xs text-muted-foreground space-y-1">
                  <div className="font-medium">Format: minute hour day month weekday</div>
                  <div>Examples:</div>
                  <ul className="list-disc list-inside space-y-0.5 ml-2">
                    <li><code>0 7 * * *</code> - Every day at 7:00 AM</li>
                    <li><code>0 7 * * 1-5</code> - Weekdays at 7:00 AM</li>
                    <li><code>0 3 * * 0</code> - Sundays at 3:00 AM</li>
                    <li><code>*/30 * * * *</code> - Every 30 minutes</li>
                  </ul>
                </div>
              </TabsContent>
            </Tabs>
          </div>

          {/* Timezone */}
          <div className="space-y-2">
            <Label htmlFor="timezone">Timezone</Label>
            <Select value={timezone} onValueChange={setTimezone}>
              <SelectTrigger>
                <SelectValue placeholder="Select timezone" />
              </SelectTrigger>
              <SelectContent>
                {COMMON_TIMEZONES.map((tz) => (
                  <SelectItem key={tz} value={tz}>
                    {tz}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Preview */}
          {effectiveCron && (
            <div className="rounded-lg border bg-muted/50 p-3 space-y-2">
              <div className="flex items-center gap-2 text-sm">
                <CheckCircle2 className="h-4 w-4 text-green-600" />
                <span className="font-medium">Schedule Preview</span>
              </div>
              <div className="text-xs space-y-1">
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="text-[10px]">
                    {operation.toUpperCase()}
                  </Badge>
                  <span>
                    {selectedPresetData?.description ||
                      `Custom: ${cronExpression}`}
                  </span>
                </div>
                <div className="flex items-center gap-1.5 text-muted-foreground">
                  <Clock className="h-3 w-3" />
                  <span>Timezone: {timezone}</span>
                </div>
                <code className="text-muted-foreground/70 block">
                  Cron: {effectiveCron}
                </code>
              </div>
            </div>
          )}
            </div>
          </ScrollArea>

          <DialogFooter className="mt-4 pt-4 border-t">
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={isPending || !name || !effectiveCron}>
              {isPending ? 'Saving...' : isEditing ? 'Save Changes' : 'Create Schedule'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
