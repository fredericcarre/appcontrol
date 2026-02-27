import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface DiscoveryReport {
  id: string;
  agent_id: string;
  hostname: string;
  scanned_at: string;
}

export interface DiscoveryReportDetail extends DiscoveryReport {
  report: {
    processes?: DiscoveredProcess[];
    listeners?: DiscoveredListener[];
    connections?: DiscoveredConnection[];
    services?: DiscoveredService[];
  };
}

export interface DiscoveredProcess {
  pid: number;
  name: string;
  cmdline: string;
  user: string;
  memory_bytes: number;
  cpu_pct: number;
  listening_ports: number[];
  env_vars?: Record<string, string>;
}

export interface DiscoveredListener {
  port: number;
  protocol: string;
  pid: number | null;
  process_name: string | null;
  address: string;
}

export interface DiscoveredConnection {
  local_port: number;
  remote_addr: string;
  remote_port: number;
  pid: number | null;
  process_name: string | null;
  state: string;
}

export interface DiscoveredService {
  name: string;
  display_name: string;
  status: string;
  pid: number | null;
}

export interface DiscoveryDraft {
  id: string;
  name: string;
  status: string;
  inferred_at: string;
}

export interface DraftComponent {
  id: string;
  name: string;
  process_name: string | null;
  host: string | null;
  component_type: string;
}

export interface DraftDependency {
  from_component: string;
  to_component: string;
  inferred_via: string;
}

export interface DraftDetail extends DiscoveryDraft {
  components: DraftComponent[];
  dependencies: DraftDependency[];
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useDiscoveryReports() {
  return useQuery<DiscoveryReport[]>({
    queryKey: ['discovery', 'reports'],
    queryFn: async () => {
      const { data } = await client.get('/v1/discovery/reports');
      return data.reports;
    },
  });
}

export function useDiscoveryReport(reportId: string | undefined) {
  return useQuery<DiscoveryReportDetail>({
    queryKey: ['discovery', 'reports', reportId],
    queryFn: async () => {
      const { data } = await client.get(`/v1/discovery/reports/${reportId}`);
      return data;
    },
    enabled: !!reportId,
  });
}

export function useDiscoveryDrafts() {
  return useQuery<DiscoveryDraft[]>({
    queryKey: ['discovery', 'drafts'],
    queryFn: async () => {
      const { data } = await client.get('/v1/discovery/drafts');
      return data.drafts;
    },
  });
}

export function useDiscoveryDraft(draftId: string | undefined) {
  return useQuery<DraftDetail>({
    queryKey: ['discovery', 'drafts', draftId],
    queryFn: async () => {
      const { data } = await client.get(`/v1/discovery/drafts/${draftId}`);
      return data;
    },
    enabled: !!draftId,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useTriggerScan() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (agentId: string) => {
      const { data } = await client.post(`/v1/discovery/trigger/${agentId}`);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['discovery', 'reports'] });
    },
  });
}

export function useTriggerAllScans() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { data } = await client.post('/v1/discovery/trigger-all');
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['discovery', 'reports'] });
    },
  });
}

export function useInferTopology() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (params: { name: string; agent_ids: string[] }) => {
      const { data } = await client.post('/v1/discovery/infer', params);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['discovery', 'drafts'] });
    },
  });
}

export function useApplyDraft() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (draftId: string) => {
      const { data } = await client.post(`/v1/discovery/drafts/${draftId}/apply`);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['discovery', 'drafts'] });
      qc.invalidateQueries({ queryKey: ['apps'] });
    },
  });
}
