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
    scheduled_jobs?: DiscoveredScheduledJob[];
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
  working_dir?: string;
  config_files?: DiscoveredConfigFile[];
  log_files?: DiscoveredLogFile[];
  command_suggestion?: CommandSuggestion;
  matched_service?: string;
}

export interface DiscoveredConfigFile {
  path: string;
  extracted_endpoints?: ExtractedEndpoint[];
}

export interface ExtractedEndpoint {
  key: string;
  value: string;
  parsed_host?: string;
  parsed_port?: number;
  technology?: string;
}

export interface DiscoveredLogFile {
  path: string;
  size_bytes: number;
}

export interface CommandSuggestion {
  check_cmd: string;
  start_cmd?: string;
  stop_cmd?: string;
  restart_cmd?: string;
  confidence: string;
  source: string;
}

export interface DiscoveredScheduledJob {
  name: string;
  schedule: string;
  command: string;
  user: string;
  source: string;
  enabled: boolean;
  hostname?: string;
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

// --- Correlation result (from POST /correlate) ---

export interface CorrelatedService {
  agent_id: string;
  hostname: string;
  process_name: string;
  ports: number[];
  port_details: Array<{ port: number; address: string; pid: number | null }>;
  suggested_name: string;
  component_type: string;
  command_suggestion?: CommandSuggestion;
  config_files?: DiscoveredConfigFile[];
  log_files?: DiscoveredLogFile[];
  matched_service?: string;
}

export interface CorrelatedDependency {
  from_service_index: number | null;
  to_service_index: number;
  from_process: string;
  to_process: string;
  remote_addr: string;
  remote_port: number;
  inferred_via: string;
  config_key?: string;
  technology?: string;
}

export interface UnresolvedConnection {
  from_hostname: string;
  from_agent_id: string;
  from_process: string;
  remote_addr: string;
  remote_port: number;
}

export interface CorrelationResult {
  agents_analyzed: number;
  services: CorrelatedService[];
  dependencies: CorrelatedDependency[];
  unresolved_connections: UnresolvedConnection[];
  scheduled_jobs: DiscoveredScheduledJob[];
}

// --- Drafts ---

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
  metadata?: Record<string, unknown>;
  check_cmd?: string;
  start_cmd?: string;
  stop_cmd?: string;
  restart_cmd?: string;
  command_confidence?: string;
  command_source?: string;
  config_files?: DiscoveredConfigFile[];
  log_files?: DiscoveredLogFile[];
  matched_service?: string;
}

export interface DraftDependency {
  id: string;
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

export function useCorrelate() {
  return useMutation<CorrelationResult, Error, { agent_ids: string[] }>({
    mutationFn: async (params) => {
      const { data } = await client.post('/v1/discovery/correlate', params);
      return data;
    },
  });
}

export function useCreateDraft() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (params: {
      name: string;
      components: Array<{
        temp_id: string;
        name: string;
        process_name: string | null;
        host: string | null;
        agent_id: string | null;
        listening_ports: number[];
        component_type: string;
        check_cmd?: string;
        start_cmd?: string;
        stop_cmd?: string;
        restart_cmd?: string;
        command_confidence?: string;
        command_source?: string;
        config_files?: unknown;
        log_files?: unknown;
        matched_service?: string;
      }>;
      dependencies: Array<{
        from_temp_id: string;
        to_temp_id: string;
        inferred_via: string;
      }>;
    }) => {
      const { data } = await client.post('/v1/discovery/drafts', params);
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
