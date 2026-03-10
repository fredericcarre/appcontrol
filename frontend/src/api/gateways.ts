import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Gateway {
  id: string;
  name: string;
  zone: string;
  status: 'active' | 'suspended';
  role: 'primary' | 'standby' | 'failover_active' | 'primary_offline' | 'standby_offline';
  is_primary: boolean;
  priority: number;
  agent_count: number;
  connected: boolean;
  version: string | null;
  last_heartbeat_at: string | null;
}

export interface ZoneSummary {
  zone: string;
  gateway_count: number;
  active_gateway_id: string | null;
  failover_active: boolean;
  gateways: Gateway[];
}

export interface GatewayAgent {
  id: string;
  hostname: string;
  is_active: boolean;
  last_heartbeat_at: string | null;
  connected: boolean;
}

export function useGatewayZones() {
  return useQuery({
    queryKey: ['gateways', 'zones'],
    queryFn: async () => {
      const { data } = await client.get<{ zones: ZoneSummary[] }>('/gateways');
      return data.zones;
    },
    refetchInterval: 10000, // Refresh every 10s to track connection status
  });
}

// Legacy hook for backwards compatibility
export function useGateways() {
  const { data: zones, ...rest } = useGatewayZones();
  // Flatten zones into a list of gateways
  const gateways = zones?.flatMap((z) => z.gateways) ?? [];
  return { data: gateways, ...rest };
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

export function useSetGatewayPrimary() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (gatewayId: string) => {
      const { data } = await client.post(`/gateways/${gatewayId}/set-primary`);
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

export interface BlockGatewayResponse {
  status: 'blocked';
  gateway_id: string;
  gateway_name: string;
  zone: string;
  agents_disconnected: number;
}

export function useBlockGateway() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (gatewayId: string) => {
      const { data } = await client.post<BlockGatewayResponse>(`/gateways/${gatewayId}/block`);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['gateways'] });
      qc.invalidateQueries({ queryKey: ['agents'] });
    },
  });
}
