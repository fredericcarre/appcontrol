import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

// ── Types ──────────────────────────────────────────────────────

export interface Schedule {
  id: string;
  name: string;
  description: string | null;
  operation: 'start' | 'stop' | 'restart';
  cron_expression: string;
  cron_human: string;
  timezone: string;
  is_enabled: boolean;
  next_run_at: string | null;
  next_run_relative: string | null;
  last_run_at: string | null;
  last_run_status: 'success' | 'failed' | 'skipped' | null;
  last_run_message: string | null;
  target_type: 'application' | 'component';
  target_id: string;
  target_name: string;
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface ScheduleExecution {
  id: string;
  schedule_id: string;
  action_log_id: string | null;
  executed_at: string;
  status: 'success' | 'failed' | 'skipped';
  message: string | null;
  duration_ms: number | null;
}

export interface SchedulePreset {
  id: string;
  label: string;
  description: string;
  cron: string;
}

export interface CreateScheduleInput {
  name: string;
  description?: string;
  operation: 'start' | 'stop' | 'restart';
  cron_expression?: string;
  preset?: string;
  timezone?: string;
}

export interface UpdateScheduleInput {
  name?: string;
  description?: string;
  operation?: 'start' | 'stop' | 'restart';
  cron_expression?: string;
  preset?: string;
  timezone?: string;
  is_enabled?: boolean;
}

// ── Application Schedules ──────────────────────────────────────

export function useAppSchedules(appId: string, includeDisabled = true) {
  return useQuery({
    queryKey: ['apps', appId, 'schedules', { includeDisabled }],
    queryFn: async () => {
      const { data } = await client.get<Schedule[]>(
        `/apps/${appId}/schedules?include_disabled=${includeDisabled}`
      );
      return data;
    },
    enabled: !!appId,
    refetchInterval: 30000, // Refresh every 30s to update next_run_relative
  });
}

export function useCreateAppSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ appId, ...payload }: CreateScheduleInput & { appId: string }) => {
      const { data } = await client.post<Schedule>(`/apps/${appId}/schedules`, payload);
      return data;
    },
    onSuccess: (_, vars) => {
      qc.invalidateQueries({ queryKey: ['apps', vars.appId, 'schedules'] });
    },
  });
}

// ── Component Schedules ────────────────────────────────────────

export function useComponentSchedules(componentId: string, includeDisabled = true) {
  return useQuery({
    queryKey: ['components', componentId, 'schedules', { includeDisabled }],
    queryFn: async () => {
      const { data } = await client.get<Schedule[]>(
        `/components/${componentId}/schedules?include_disabled=${includeDisabled}`
      );
      return data;
    },
    enabled: !!componentId,
    refetchInterval: 30000,
  });
}

export function useCreateComponentSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ componentId, ...payload }: CreateScheduleInput & { componentId: string }) => {
      const { data } = await client.post<Schedule>(`/components/${componentId}/schedules`, payload);
      return data;
    },
    onSuccess: (_, vars) => {
      qc.invalidateQueries({ queryKey: ['components', vars.componentId, 'schedules'] });
    },
  });
}

// ── Individual Schedule Operations ─────────────────────────────

export function useSchedule(scheduleId: string) {
  return useQuery({
    queryKey: ['schedules', scheduleId],
    queryFn: async () => {
      const { data } = await client.get<Schedule>(`/schedules/${scheduleId}`);
      return data;
    },
    enabled: !!scheduleId,
  });
}

export function useUpdateSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, appId, componentId, ...payload }: UpdateScheduleInput & {
      id: string;
      appId?: string;
      componentId?: string
    }) => {
      const { data } = await client.put<Schedule>(`/schedules/${id}`, payload);
      return { data, appId, componentId };
    },
    onSuccess: (result) => {
      qc.invalidateQueries({ queryKey: ['schedules', result.data.id] });
      if (result.appId) {
        qc.invalidateQueries({ queryKey: ['apps', result.appId, 'schedules'] });
      }
      if (result.componentId) {
        qc.invalidateQueries({ queryKey: ['components', result.componentId, 'schedules'] });
      }
    },
  });
}

export function useDeleteSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, appId, componentId }: {
      id: string;
      appId?: string;
      componentId?: string
    }) => {
      await client.delete(`/schedules/${id}`);
      return { appId, componentId };
    },
    onSuccess: (result) => {
      if (result.appId) {
        qc.invalidateQueries({ queryKey: ['apps', result.appId, 'schedules'] });
      }
      if (result.componentId) {
        qc.invalidateQueries({ queryKey: ['components', result.componentId, 'schedules'] });
      }
    },
  });
}

export function useToggleSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, appId, componentId }: {
      id: string;
      appId?: string;
      componentId?: string
    }) => {
      const { data } = await client.post<Schedule>(`/schedules/${id}/toggle`);
      return { data, appId, componentId };
    },
    onSuccess: (result) => {
      qc.invalidateQueries({ queryKey: ['schedules', result.data.id] });
      if (result.appId) {
        qc.invalidateQueries({ queryKey: ['apps', result.appId, 'schedules'] });
      }
      if (result.componentId) {
        qc.invalidateQueries({ queryKey: ['components', result.componentId, 'schedules'] });
      }
    },
  });
}

export function useRunScheduleNow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, appId, componentId }: {
      id: string;
      appId?: string;
      componentId?: string
    }) => {
      const { data } = await client.post<{ message: string; schedule_id: string }>(`/schedules/${id}/run-now`);
      return { data, appId, componentId };
    },
    onSuccess: (result) => {
      // Give the scheduler time to pick it up, then refresh
      setTimeout(() => {
        if (result.appId) {
          qc.invalidateQueries({ queryKey: ['apps', result.appId, 'schedules'] });
        }
        if (result.componentId) {
          qc.invalidateQueries({ queryKey: ['components', result.componentId, 'schedules'] });
        }
      }, 2000);
    },
  });
}

// ── Schedule Execution History ─────────────────────────────────

export function useScheduleExecutions(scheduleId: string) {
  return useQuery({
    queryKey: ['schedules', scheduleId, 'executions'],
    queryFn: async () => {
      const { data } = await client.get<ScheduleExecution[]>(`/schedules/${scheduleId}/executions`);
      return data;
    },
    enabled: !!scheduleId,
  });
}

// ── Presets ────────────────────────────────────────────────────

export function useSchedulePresets() {
  return useQuery({
    queryKey: ['schedules', 'presets'],
    queryFn: async () => {
      const { data } = await client.get<SchedulePreset[]>('/schedules/presets');
      return data;
    },
    staleTime: Infinity, // Presets don't change
  });
}

// ── Utility Functions ──────────────────────────────────────────

export function getOperationColor(operation: Schedule['operation']): string {
  switch (operation) {
    case 'start':
      return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200';
    case 'stop':
      return 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200';
    case 'restart':
      return 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200';
    default:
      return 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-200';
  }
}

export function getStatusColor(status: Schedule['last_run_status']): string {
  switch (status) {
    case 'success':
      return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200';
    case 'failed':
      return 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200';
    case 'skipped':
      return 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200';
    default:
      return 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-200';
  }
}
