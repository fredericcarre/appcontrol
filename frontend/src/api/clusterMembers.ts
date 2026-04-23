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
