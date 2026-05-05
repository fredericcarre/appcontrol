import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface ManualTaskValidation {
  id: string;
  component_id: string;
  application_id: string;
  started_at: string;
  started_by: string | null;
  validated_at: string | null;
  validated_by: string | null;
  status: 'pending' | 'validated' | 'skipped' | 'failed';
  comment: string | null;
  duration_seconds: number | null;
}

export interface ManualTaskResponse {
  component_id: string;
  manual_description: string | null;
  history: ManualTaskValidation[];
}

const key = (componentId: string) => ['manual-task', componentId] as const;

/**
 * Polls the manual-task endpoint when there's a pending validation, so
 * the panel reflects the current state during a Start operation. Drops
 * back to 30 s polling once nothing is pending.
 */
export function useManualTask(componentId: string | undefined) {
  return useQuery({
    queryKey: componentId ? key(componentId) : ['manual-task', '_none'],
    queryFn: async () => {
      const { data } = await client.get<ManualTaskResponse>(
        `/components/${componentId}/manual-task`,
      );
      return data;
    },
    enabled: !!componentId,
    refetchInterval: (query) => {
      const data = query.state.data as ManualTaskResponse | undefined;
      const hasPending = data?.history.some((h) => h.status === 'pending');
      return hasPending ? 2000 : 30000;
    },
  });
}

export interface PendingManualTask {
  validation_id: string;
  component_id: string;
  component_name: string;
  component_display_name: string | null;
  application_id: string;
  application_name: string;
  started_at: string;
  manual_description: string | null;
}

export interface PendingManualTasksResponse {
  tasks: PendingManualTask[];
  count: number;
}

/**
 * Cross-app inbox of pending manual tasks for the current user. Drives the
 * dashboard notification widget — fetched on a 15s interval so newly-paused
 * sequencers surface within a quarter-minute.
 */
export function usePendingManualTasks() {
  return useQuery({
    queryKey: ['manual-task', 'pending', 'me'],
    queryFn: async () => {
      const { data } = await client.get<PendingManualTasksResponse>(
        '/me/pending-manual-tasks',
      );
      return data;
    },
    refetchInterval: 15000,
  });
}

export function useValidateManualTask(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      status: 'validated' | 'skipped' | 'failed';
      comment?: string;
    }) => {
      const { data } = await client.post(
        `/components/${componentId}/manual-task/validate`,
        payload,
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: key(componentId) });
    },
  });
}
