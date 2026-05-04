import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface ClusterMember {
  id: string;
  component_id: string;
  hostname: string;
  agent_id: string;
  site_id: string | null;
  check_cmd_override: string | null;
  start_cmd_override: string | null;
  stop_cmd_override: string | null;
  install_path: string | null;
  env_vars_override: Record<string, unknown> | null;
  member_order: number;
  is_enabled: boolean;
  tags: string[] | Record<string, unknown>;
  created_at: string;
  updated_at: string;
  // Present on list (joined with cluster_member_state)
  current_state?: string;
  last_check_at?: string | null;
  last_check_exit_code?: number | null;
}

export interface CreateClusterMemberPayload {
  hostname: string;
  agent_id: string;
  site_id?: string | null;
  check_cmd_override?: string | null;
  start_cmd_override?: string | null;
  stop_cmd_override?: string | null;
  install_path?: string | null;
  env_vars_override?: Record<string, unknown> | null;
  member_order?: number;
  is_enabled?: boolean;
  tags?: unknown;
}

export interface UpdateClusterMemberPayload {
  hostname?: string;
  agent_id?: string;
  site_id?: string | null;
  check_cmd_override?: string | null;
  start_cmd_override?: string | null;
  stop_cmd_override?: string | null;
  install_path?: string | null;
  env_vars_override?: Record<string, unknown> | null;
  member_order?: number;
  is_enabled?: boolean;
  tags?: unknown;
}

export interface BatchActionPayload {
  member_ids?: string[];
  parallel?: boolean;
}

export interface BatchActionResponse {
  action: 'start' | 'stop';
  dispatched: Array<{
    member_id: string;
    agent_id: string;
    request_id: string;
  }>;
  skipped: Array<{ member_id: string; reason: string }>;
}

const membersKey = (componentId: string) => ['cluster-members', componentId] as const;

export function useClusterMembers(componentId: string | null | undefined, enabled = true) {
  return useQuery({
    queryKey: componentId ? membersKey(componentId) : ['cluster-members', '_none'],
    queryFn: async () => {
      const { data } = await client.get<{ members: ClusterMember[] }>(
        `/components/${componentId}/members`,
      );
      return data.members;
    },
    enabled: !!componentId && enabled,
  });
}

const appMembersKey = (appId: string) => ['cluster-members', 'by-app', appId] as const;

/**
 * Fetch every fan-out member of an entire application in one round-trip.
 * Used by the map view in "expanded" mode (each member rendered as its own
 * sub-node), so we don't fan out N parallel `useClusterMembers` requests.
 */
export function useAppClusterMembers(appId: string | null | undefined, enabled = true) {
  return useQuery({
    queryKey: appId ? appMembersKey(appId) : ['cluster-members', 'by-app', '_none'],
    queryFn: async () => {
      const { data } = await client.get<{ members: ClusterMember[] }>(
        `/apps/${appId}/cluster-members`,
      );
      return data.members;
    },
    enabled: !!appId && enabled,
    refetchInterval: 5000, // members change state often during start/stop sequences
  });
}

export function useCreateClusterMember(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: CreateClusterMemberPayload) => {
      const { data } = await client.post<ClusterMember>(
        `/components/${componentId}/members`,
        payload,
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: membersKey(componentId) });
    },
  });
}

export function useUpdateClusterMember(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (args: { id: string; payload: UpdateClusterMemberPayload }) => {
      const { data } = await client.put<ClusterMember>(
        `/members/${args.id}`,
        args.payload,
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: membersKey(componentId) });
    },
  });
}

export function useDeleteClusterMember(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      await client.delete(`/members/${id}`);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: membersKey(componentId) });
    },
  });
}

export function useBatchStartMembers(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: BatchActionPayload = {}) => {
      const { data } = await client.post<BatchActionResponse>(
        `/components/${componentId}/members/actions/start`,
        payload,
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: membersKey(componentId) });
    },
  });
}

export function useBatchStopMembers(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: BatchActionPayload = {}) => {
      const { data } = await client.post<BatchActionResponse>(
        `/components/${componentId}/members/actions/stop`,
        payload,
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: membersKey(componentId) });
    },
  });
}

/**
 * Start or stop a single fan-out member when the calling code only knows
 * the member id, not its parent component id (e.g. the map's expanded view
 * triggers a per-node start). Internally dispatches the existing batch
 * endpoint with `member_ids: [memberId]`, scoped to `componentId`.
 */
export function useMemberAction() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (args: {
      componentId: string;
      memberId: string;
      action: 'start' | 'stop';
    }) => {
      const { data } = await client.post<BatchActionResponse>(
        `/components/${args.componentId}/members/actions/${args.action}`,
        { member_ids: [args.memberId] },
      );
      return data;
    },
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: membersKey(vars.componentId) });
      qc.invalidateQueries({ queryKey: ['cluster-members', 'by-app'] });
    },
  });
}

export interface UpdateClusterConfigPayload {
  cluster_mode?: 'aggregate' | 'fan_out';
  cluster_health_policy?: 'all_healthy' | 'any_healthy' | 'quorum' | 'threshold_pct';
  cluster_min_healthy_pct?: number;
}

export function useUpdateClusterConfig(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: UpdateClusterConfigPayload) => {
      const { data } = await client.put(
        `/components/${componentId}/cluster-config`,
        payload,
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: membersKey(componentId) });
    },
  });
}
