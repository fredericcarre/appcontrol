import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface AvailabilityReport {
  app_id: string;
  app_name: string;
  period_start: string;
  period_end: string;
  uptime_percent: number;
  total_downtime_minutes: number;
  incidents: number;
}

export interface AuditEntry {
  id: string;
  action: string;
  user_email: string;
  target_type: string;
  target_id: string;
  target_name?: string;
  details: Record<string, unknown>;
  created_at: string;
}

export function useAvailabilityReport(appId: string, period: string) {
  return useQuery({
    queryKey: ['reports', 'availability', appId, period],
    queryFn: async () => {
      const { data } = await client.get<AvailabilityReport>(`/apps/${appId}/reports/availability`, { params: { period } });
      return data;
    },
    enabled: !!appId,
  });
}

export function useIncidentReport(appId: string, period: string) {
  return useQuery({
    queryKey: ['reports', 'incidents', appId, period],
    queryFn: async () => {
      const { data } = await client.get(`/reports/incidents/${appId}`, { params: { period } });
      return data;
    },
    enabled: !!appId,
  });
}

export function useAuditLog(params: { app_id?: string; user_id?: string; limit?: number; offset?: number }) {
  return useQuery({
    queryKey: ['reports', 'audit', params],
    queryFn: async () => {
      const { data } = await client.get<AuditEntry[]>('/reports/audit', { params });
      return data;
    },
  });
}

export interface ComplianceReport {
  report: string;
  dora_compliant: boolean;
  audit_trail_entries: number;
  append_only_enforced: boolean;
}

export function useComplianceReport(appId: string) {
  return useQuery({
    queryKey: ['reports', 'compliance', appId],
    queryFn: async () => {
      const { data } = await client.get<ComplianceReport>(`/apps/${appId}/reports/compliance`);
      return data;
    },
    enabled: !!appId,
  });
}

export interface Agent {
  id: string;
  hostname: string;
  status: string;
  version: string;
  last_heartbeat: string;
  last_heartbeat_at: string | null;
  gateway_id: string | null;
  gateway_name: string | null;
  gateway_zone: string | null;
  connected: boolean;
  gateway_connected: boolean;
  is_active: boolean;
}

export function useAgents() {
  return useQuery({
    queryKey: ['agents'],
    queryFn: async () => {
      const { data } = await client.get<{ agents: Agent[] }>('/agents');
      return data.agents;
    },
  });
}

export function useBulkDeleteAgents() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (agentIds: string[]) => {
      const { data } = await client.post('/agents/bulk-delete', { agent_ids: agentIds });
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['agents'] });
    },
  });
}
