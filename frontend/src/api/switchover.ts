import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface SwitchoverPhase {
  phase: string;
  status: string;
  at: string;
}

export interface SwitchoverStatus {
  switchover_id: string | null;
  current_phase: string;
  current_status: string;
  history: SwitchoverPhase[];
}

export interface StartSwitchoverRequest {
  target_site_id: string;
  mode: 'FULL' | 'SELECTIVE' | 'PROGRESSIVE';
  component_ids?: string[];
}

export interface SwitchoverResponse {
  switchover_id: string;
  phase: string;
  status: string;
  previous_phase?: string;
  current_phase?: string;
  details?: Record<string, unknown>;
}

/**
 * Get the current switchover status for an application
 */
export function useSwitchoverStatus(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'switchover', 'status'],
    queryFn: async () => {
      const { data } = await client.get<SwitchoverStatus>(
        `/apps/${appId}/switchover/status`
      );
      return data;
    },
    enabled: !!appId,
    refetchInterval: 5000, // Poll every 5 seconds during active switchover
  });
}

/**
 * Start a new switchover to a target site
 */
export function useStartSwitchover() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ appId, ...payload }: StartSwitchoverRequest & { appId: string }) => {
      const { data } = await client.post<SwitchoverResponse>(
        `/apps/${appId}/switchover`,
        payload
      );
      return data;
    },
    onSuccess: (_, variables) => {
      qc.invalidateQueries({ queryKey: ['apps', variables.appId, 'switchover'] });
    },
  });
}

/**
 * Advance to the next switchover phase
 */
export function useAdvanceSwitchover() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post<SwitchoverResponse>(
        `/apps/${appId}/switchover/next-phase`
      );
      return data;
    },
    onSuccess: (_, appId) => {
      qc.invalidateQueries({ queryKey: ['apps', appId, 'switchover'] });
      qc.invalidateQueries({ queryKey: ['apps', appId] });
      qc.invalidateQueries({ queryKey: ['apps', appId, 'components'] });
      qc.invalidateQueries({ queryKey: ['apps', appId, 'bindings'] });
    },
  });
}

/**
 * Rollback the current switchover
 */
export function useRollbackSwitchover() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post<SwitchoverResponse>(
        `/apps/${appId}/switchover/rollback`
      );
      return data;
    },
    onSuccess: (_, appId) => {
      qc.invalidateQueries({ queryKey: ['apps', appId, 'switchover'] });
      qc.invalidateQueries({ queryKey: ['apps', appId] });
      qc.invalidateQueries({ queryKey: ['apps', appId, 'components'] });
      qc.invalidateQueries({ queryKey: ['apps', appId, 'bindings'] });
    },
  });
}

/**
 * Commit the switchover (final phase)
 */
export function useCommitSwitchover() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post<SwitchoverResponse>(
        `/apps/${appId}/switchover/commit`
      );
      return data;
    },
    onSuccess: (_, appId) => {
      qc.invalidateQueries({ queryKey: ['apps', appId, 'switchover'] });
      qc.invalidateQueries({ queryKey: ['apps', appId] });
      qc.invalidateQueries({ queryKey: ['apps', appId, 'components'] });
      qc.invalidateQueries({ queryKey: ['apps', appId, 'bindings'] });
    },
  });
}
