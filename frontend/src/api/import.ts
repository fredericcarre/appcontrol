import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export interface ImportPreviewRequest {
  content: string;
  format: 'json' | 'yaml';
  gateway_ids: string[];
  dr_gateway_ids?: string[];
}

export interface ComponentResolution {
  name: string;
  host: string | null;
  component_type: string;
  resolution: ComponentResolutionStatus;
}

export type ComponentResolutionStatus =
  | { status: 'resolved'; agent_id: string; agent_hostname: string; gateway_id: string | null; gateway_name: string | null; resolved_via: string }
  | { status: 'multiple'; candidates: AgentCandidate[] }
  | { status: 'unresolved' }
  | { status: 'no_host' };

export interface AgentCandidate {
  agent_id: string;
  hostname: string;
  gateway_id: string | null;
  gateway_name: string | null;
  ip_addresses: string[];
  matched_via: string;
}

export interface AvailableAgent {
  agent_id: string;
  hostname: string;
  gateway_id: string | null;
  gateway_name: string | null;
  ip_addresses: string[];
  is_active: boolean;
}

export interface DrSuggestion {
  component_name: string;
  primary_host: string;
  suggested_dr_host: string | null;
  dr_resolution: ComponentResolutionStatus | null;
}

export interface ExistingApplicationInfo {
  id: string;
  name: string;
  component_count: number;
  created_at: string;
}

export interface ImportPreviewResponse {
  valid: boolean;
  application_name: string;
  component_count: number;
  all_resolved: boolean;
  components: ComponentResolution[];
  available_agents: AvailableAgent[];
  dr_available_agents: AvailableAgent[] | null;
  dr_suggestions: DrSuggestion[] | null;
  warnings: string[];
  existing_application: ExistingApplicationInfo | null;
}

export interface MappingConfig {
  component_name: string;
  agent_id: string;
  resolved_via: string;
}

// Site override for DR/failover configuration
export interface SiteOverrideConfig {
  site_code: string;
  host_override?: string;
  check_cmd_override?: string;
  start_cmd_override?: string;
  stop_cmd_override?: string;
  rebuild_cmd_override?: string;
  env_vars_override?: Record<string, string>;
}

export interface ProfileConfig {
  name: string;
  description?: string;
  profile_type: 'primary' | 'dr' | 'custom';
  gateway_ids: string[];
  auto_failover?: boolean;
  mappings: MappingConfig[];
}

export type ConflictAction = 'fail' | 'rename' | 'update';

export interface ImportExecuteRequest {
  content: string;
  format: 'json' | 'yaml';
  site_id?: string;  // Optional - backend auto-selects default site if not provided
  profile: ProfileConfig;
  dr_profile?: ProfileConfig;
  conflict_action?: ConflictAction;  // How to handle name conflicts (default: fail)
  new_name?: string;  // Required if conflict_action is 'rename'
}

export interface ImportExecuteResponse {
  application_id: string;
  application_name: string;
  components_created: number;
  profiles_created: string[];
  active_profile: string;
  warnings: string[];
}

// ═══════════════════════════════════════════════════════════════════════════
// Profile Types
// ═══════════════════════════════════════════════════════════════════════════

export interface BindingProfile {
  id: string;
  application_id: string;
  name: string;
  description: string | null;
  profile_type: string;
  is_active: boolean;
  gateway_ids: string[];
  auto_failover: boolean;
  mapping_count: number;
  created_at: string;
}

export interface ProfileMapping {
  id: string;
  profile_id: string;
  component_name: string;
  host: string;
  agent_id: string;
  resolved_via: string;
}

export interface DrPatternRule {
  id: string;
  organization_id: string;
  name: string;
  search_pattern: string;
  replace_pattern: string;
  priority: number;
  is_active: boolean;
  created_at: string;
}

// ═══════════════════════════════════════════════════════════════════════════
// Import Preview & Execute Hooks
// ═══════════════════════════════════════════════════════════════════════════

export function useImportPreview() {
  return useMutation({
    mutationFn: async (payload: ImportPreviewRequest) => {
      const { data } = await client.post<ImportPreviewResponse>('/import/preview', payload);
      return data;
    },
  });
}

export function useImportExecute() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: ImportExecuteRequest) => {
      const { data } = await client.post<ImportExecuteResponse>('/import/execute', payload);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['apps'] });
    },
  });
}

// ═══════════════════════════════════════════════════════════════════════════
// Profile Management Hooks
// ═══════════════════════════════════════════════════════════════════════════

export function useProfiles(appId: string) {
  return useQuery({
    queryKey: ['profiles', appId],
    queryFn: async () => {
      const { data } = await client.get<BindingProfile[]>(`/apps/${appId}/profiles`);
      return data;
    },
    enabled: !!appId,
  });
}

export function useProfile(appId: string, name: string) {
  return useQuery({
    queryKey: ['profiles', appId, name],
    queryFn: async () => {
      const { data } = await client.get<{ profile: BindingProfile; mappings: ProfileMapping[] }>(
        `/apps/${appId}/profiles/${name}`
      );
      return data;
    },
    enabled: !!appId && !!name,
  });
}

export function useCreateProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ appId, ...payload }: { appId: string; name: string; description?: string; profile_type: string; gateway_ids: string[]; auto_failover?: boolean; copy_from_profile_id?: string; mappings?: MappingConfig[] }) => {
      const { data } = await client.post<BindingProfile>(`/apps/${appId}/profiles`, payload);
      return data;
    },
    onSuccess: (_, { appId }) => {
      qc.invalidateQueries({ queryKey: ['profiles', appId] });
    },
  });
}

export function useActivateProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ appId, name }: { appId: string; name: string }) => {
      const { data } = await client.put<{ message: string; profile: string; switchover_id: string }>(
        `/apps/${appId}/profiles/${name}/activate`
      );
      return data;
    },
    onSuccess: (_, { appId }) => {
      qc.invalidateQueries({ queryKey: ['profiles', appId] });
      qc.invalidateQueries({ queryKey: ['apps', appId] });
    },
  });
}

export function useDeleteProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ appId, name }: { appId: string; name: string }) => {
      await client.delete(`/apps/${appId}/profiles/${name}`);
    },
    onSuccess: (_, { appId }) => {
      qc.invalidateQueries({ queryKey: ['profiles', appId] });
    },
  });
}

// ═══════════════════════════════════════════════════════════════════════════
// DR Pattern Rules Hooks
// ═══════════════════════════════════════════════════════════════════════════

export function useDrPatternRules() {
  return useQuery({
    queryKey: ['dr-pattern-rules'],
    queryFn: async () => {
      const { data } = await client.get<DrPatternRule[]>('/dr-pattern-rules');
      return data;
    },
  });
}

export function useCreateDrPatternRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { name: string; search_pattern: string; replace_pattern: string; priority?: number; is_active?: boolean }) => {
      const { data } = await client.post<DrPatternRule>('/dr-pattern-rules', payload);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['dr-pattern-rules'] });
    },
  });
}

export function useUpdateDrPatternRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, ...payload }: { id: string; name: string; search_pattern: string; replace_pattern: string; priority?: number; is_active?: boolean }) => {
      const { data } = await client.put<DrPatternRule>(`/dr-pattern-rules/${id}`, payload);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['dr-pattern-rules'] });
    },
  });
}

export function useDeleteDrPatternRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      await client.delete(`/dr-pattern-rules/${id}`);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['dr-pattern-rules'] });
    },
  });
}
