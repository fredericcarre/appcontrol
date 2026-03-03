import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Agent {
  id: string;
  hostname: string;
  organization_id: string;
  gateway_id: string | null;
  labels: Record<string, string>;
  ip_addresses: string[];
  version: string | null;
  // System info
  os_name: string | null;
  os_version: string | null;
  cpu_arch: string | null;
  cpu_cores: number | null;
  total_memory_mb: number | null;
  disk_total_gb: number | null;
  // Timestamps
  last_heartbeat_at: string | null;
  is_active: boolean;
  status: 'active' | 'suspended' | 'deleted';
  connected: boolean;
  created_at: string;
  // Gateway info (from JOIN)
  gateway_name: string | null;
  gateway_zone: string | null;
  gateway_connected: boolean;
}

export function useAgents() {
  return useQuery({
    queryKey: ['agents'],
    queryFn: async () => {
      const { data } = await client.get<{ agents: Agent[] }>('/agents');
      return data.agents;
    },
    refetchInterval: 10000, // Refresh every 10s to track connection status
  });
}

export function useAgent(agentId: string) {
  return useQuery({
    queryKey: ['agents', agentId],
    queryFn: async () => {
      const { data } = await client.get<Agent>(`/agents/${agentId}`);
      return data;
    },
    enabled: !!agentId,
  });
}

export function useSuspendAgent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (agentId: string) => {
      const { data } = await client.post(`/agents/${agentId}/suspend`);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['agents'] }),
  });
}

export function useActivateAgent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (agentId: string) => {
      const { data } = await client.post(`/agents/${agentId}/activate`);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['agents'] }),
  });
}

export function useDeleteAgent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (agentId: string) => {
      await client.delete(`/agents/${agentId}`);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['agents'] }),
  });
}

export interface BlockAgentResponse {
  status: 'blocked';
  agent_id: string;
  hostname: string;
}

export function useBlockAgent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (agentId: string) => {
      const { data } = await client.post<BlockAgentResponse>(`/agents/${agentId}/block`);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['agents'] });
      qc.invalidateQueries({ queryKey: ['gateways'] });
    },
  });
}

export interface UnblockAgentResponse {
  status: 'unblocked';
  agent_id: string;
  hostname: string;
}

export function useUnblockAgent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (agentId: string) => {
      const { data } = await client.post<UnblockAgentResponse>(`/agents/${agentId}/unblock`);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['agents'] });
      qc.invalidateQueries({ queryKey: ['gateways'] });
    },
  });
}

// Metrics types and hook
export interface MetricPoint {
  cpu_pct: number;
  memory_pct: number;
  disk_used_pct: number | null;
  created_at: string;
}

export interface AgentMetricsResponse {
  agent_id: string;
  minutes: number;
  metrics: MetricPoint[];
}

export function useAgentMetrics(agentId: string, minutes: number = 60) {
  return useQuery({
    queryKey: ['agent-metrics', agentId, minutes],
    queryFn: async () => {
      const { data } = await client.get<AgentMetricsResponse>(
        `/agents/${agentId}/metrics?minutes=${minutes}`
      );
      return data;
    },
    enabled: !!agentId,
    refetchInterval: 30000, // Refresh every 30s
  });
}
