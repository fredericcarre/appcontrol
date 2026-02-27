import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Gateway {
  id: string;
  name: string;
  zone: string;
  status: 'active' | 'suspended' | 'deleted';
  agent_count: number;
  connected: boolean;
}

export interface GatewayAgent {
  id: string;
  hostname: string;
  is_active: boolean;
  last_heartbeat_at: string | null;
}

export function useGateways() {
  return useQuery({
    queryKey: ['gateways'],
    queryFn: async () => {
      const { data } = await client.get<{ gateways: Gateway[] }>('/gateways');
      return data.gateways;
    },
    refetchInterval: 10000, // Refresh every 10s to track connection status
  });
}

export function useGateway(gatewayId: string) {
  return useQuery({
    queryKey: ['gateways', gatewayId],
    queryFn: async () => {
      const { data } = await client.get<Gateway>(`/gateways/${gatewayId}`);
      return data;
    },
    enabled: !!gatewayId,
  });
}

export function useGatewayAgents(gatewayId: string) {
  return useQuery({
    queryKey: ['gateways', gatewayId, 'agents'],
    queryFn: async () => {
      const { data } = await client.get<{ agents: GatewayAgent[] }>(`/gateways/${gatewayId}/agents`);
      return data.agents;
    },
    enabled: !!gatewayId,
    refetchInterval: 10000,
  });
}

export function useSuspendGateway() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (gatewayId: string) => {
      const { data } = await client.post(`/gateways/${gatewayId}/suspend`);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['gateways'] }),
  });
}

export function useActivateGateway() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (gatewayId: string) => {
      const { data } = await client.post(`/gateways/${gatewayId}/activate`);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['gateways'] }),
  });
}

export function useDeleteGateway() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (gatewayId: string) => {
      await client.delete(`/gateways/${gatewayId}`);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['gateways'] }),
  });
}
