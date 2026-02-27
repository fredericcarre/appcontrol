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
  last_heartbeat_at: string | null;
  is_active: boolean;
  status: 'active' | 'suspended' | 'deleted';
  connected: boolean;
  created_at: string;
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
