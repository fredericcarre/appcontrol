import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Application {
  id: string;
  name: string;
  description: string;
  org_id: string;
  site_id: string | null;
  dr_site_id: string | null;
  weather: string;
  global_state: string;
  component_count: number;
  running_count: number;
  starting_count?: number;
  stopping_count?: number;
  stopped_count: number;
  failed_count: number;
  unreachable_count?: number;
  is_suspended?: boolean;
  suspended_at?: string | null;
  suspended_by?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ApplicationDetail extends Application {
  components: Component[];
  dependencies: Dependency[];
}

export interface Component {
  id: string;
  application_id: string;
  name: string;
  display_name: string | null;
  description: string | null;
  icon: string | null;
  group_id: string | null;
  host: string | null;
  component_type: string;
  current_state: string;
  check_cmd: string | null;
  start_cmd: string | null;
  stop_cmd: string | null;
  check_interval_seconds: number;
  start_timeout_seconds: number;
  stop_timeout_seconds: number;
  agent_id: string | null;
  is_optional: boolean;
  position_x: number | null;
  position_y: number | null;
  // Cluster fields
  cluster_size?: number | null;
  cluster_nodes?: string[] | null;
  // Application reference (for app-type synthetic components)
  referenced_app_id?: string | null;
  referenced_app_name?: string | null;
  created_at: string;
  updated_at: string;
  // Connectivity status (from enriched API response)
  agent_hostname?: string | null;
  agent_connected?: boolean;
  gateway_id?: string | null;
  gateway_name?: string | null;
  gateway_connected?: boolean;
  connectivity_status?: 'connected' | 'agent_disconnected' | 'gateway_disconnected' | 'no_agent';
  // Latest metrics from check command
  last_check_metrics?: Record<string, unknown> | null;
}

export interface ComponentGroup {
  id: string;
  application_id: string;
  name: string;
  description: string | null;
  color: string | null;
  display_order: number;
}

export interface AppVariable {
  id: string;
  application_id: string;
  name: string;
  value: string;
  description: string | null;
  is_secret: boolean;
}

export interface ComponentLink {
  id: string;
  component_id: string;
  label: string;
  url: string;
  link_type: string;
  display_order: number;
}

export interface CommandInputParam {
  id: string;
  command_id: string;
  name: string;
  description: string | null;
  default_value: string | null;
  validation_regex: string | null;
  required: boolean;
  display_order: number;
  param_type: string;
  enum_values: string[] | null;
}

export interface Dependency {
  id: string;
  from_component_id: string;
  to_component_id: string;
  dep_type: string;
}

export function useApps() {
  return useQuery({
    queryKey: ['apps'],
    queryFn: async () => {
      const { data } = await client.get<{ apps: Application[]; total: number }>('/apps');
      return data.apps;
    },
  });
}

export function useApp(appId: string) {
  return useQuery({
    queryKey: ['apps', appId],
    queryFn: async () => {
      const { data } = await client.get<ApplicationDetail>(`/apps/${appId}`);
      return data;
    },
    enabled: !!appId,
  });
}

export function useCreateApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { name: string; description: string; site_id?: string }) => {
      const { data } = await client.post<Application>('/apps', payload);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['apps'] }),
  });
}

export function useUpdateApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, ...payload }: { id: string; name?: string; description?: string }) => {
      const { data } = await client.put<Application>(`/apps/${id}`, payload);
      return data;
    },
    onSuccess: (_, vars) => {
      qc.invalidateQueries({ queryKey: ['apps'] });
      qc.invalidateQueries({ queryKey: ['apps', vars.id] });
    },
  });
}

export function useDeleteApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      await client.delete(`/apps/${id}`);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['apps'] }),
  });
}

export function useSuspendApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { data } = await client.put(`/apps/${id}/suspend`);
      return data;
    },
    onSuccess: (_, id) => {
      qc.invalidateQueries({ queryKey: ['apps'] });
      qc.invalidateQueries({ queryKey: ['apps', id] });
    },
  });
}

export function useResumeApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { data } = await client.put(`/apps/${id}/resume`);
      return data;
    },
    onSuccess: (_, id) => {
      qc.invalidateQueries({ queryKey: ['apps'] });
      qc.invalidateQueries({ queryKey: ['apps', id] });
    },
  });
}

export function useStartApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post(`/apps/${appId}/start`, {});
      return data;
    },
    onSuccess: (_, appId) => qc.invalidateQueries({ queryKey: ['apps', appId] }),
  });
}

export function useStopApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post(`/apps/${appId}/stop`, {});
      return data;
    },
    onSuccess: (_, appId) => qc.invalidateQueries({ queryKey: ['apps', appId] }),
  });
}

export function useCancelOperation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post(`/apps/${appId}/cancel`, {});
      return data;
    },
    onSuccess: (_, appId) => qc.invalidateQueries({ queryKey: ['apps', appId] }),
  });
}

export function useStartBranch() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { appId: string; componentId?: string }) => {
      const body: Record<string, unknown> = {};
      if (payload.componentId) body.component_id = payload.componentId;
      const { data } = await client.post(`/apps/${payload.appId}/start-branch`, body);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.appId] }),
  });
}

export function useStartTo() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { appId: string; targetComponentId: string; dryRun?: boolean }) => {
      const { data } = await client.post(`/apps/${payload.appId}/start-to`, {
        target_component_id: payload.targetComponentId,
        dry_run: payload.dryRun,
      });
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.appId] }),
  });
}

export function useAppComponents(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'components'],
    queryFn: async () => {
      const { data } = await client.get<Component[]>(`/apps/${appId}/components`);
      return data;
    },
    enabled: !!appId,
  });
}

export function useCreateComponent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: Partial<Component> & { app_id: string; name: string; host?: string; agent_id?: string }) => {
      const { data } = await client.post<Component>(`/apps/${payload.app_id}/components`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id] }),
  });
}

export function useAddDependency() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; from_component_id: string; to_component_id: string; dep_type?: string }) => {
      const { data } = await client.post(`/apps/${payload.app_id}/dependencies`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id] }),
  });
}

// ── Variables ──────────────────────────────────────────────────

export function useAppVariables(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'variables'],
    queryFn: async () => {
      const { data } = await client.get<{ variables: AppVariable[] }>(`/apps/${appId}/variables`);
      return data.variables;
    },
    enabled: !!appId,
  });
}

export function useCreateVariable() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; name: string; value: string; description?: string; is_secret?: boolean }) => {
      const { data } = await client.post(`/apps/${payload.app_id}/variables`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'variables'] }),
  });
}

export function useUpdateVariable() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; var_id: string; value?: string; description?: string; is_secret?: boolean }) => {
      const { data } = await client.put(`/apps/${payload.app_id}/variables/${payload.var_id}`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'variables'] }),
  });
}

export function useDeleteVariable() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; var_id: string }) => {
      await client.delete(`/apps/${payload.app_id}/variables/${payload.var_id}`);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'variables'] }),
  });
}

// ── Component Groups ───────────────────────────────────────────

export function useComponentGroups(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'groups'],
    queryFn: async () => {
      const { data } = await client.get<{ groups: ComponentGroup[] }>(`/apps/${appId}/groups`);
      return data.groups;
    },
    enabled: !!appId,
  });
}

export function useCreateGroup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; name: string; description?: string; color?: string }) => {
      const { data } = await client.post(`/apps/${payload.app_id}/groups`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'groups'] }),
  });
}

export function useDeleteGroup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; group_id: string }) => {
      await client.delete(`/apps/${payload.app_id}/groups/${payload.group_id}`);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'groups'] }),
  });
}

// ── Component Links ────────────────────────────────────────────

export function useComponentLinks(componentId: string) {
  return useQuery({
    queryKey: ['components', componentId, 'links'],
    queryFn: async () => {
      const { data } = await client.get<{ links: ComponentLink[] }>(`/components/${componentId}/links`);
      return data.links;
    },
    enabled: !!componentId,
  });
}

export function useCreateLink() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { component_id: string; label: string; url: string; link_type?: string }) => {
      const { data } = await client.post(`/components/${payload.component_id}/links`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['components', vars.component_id, 'links'] }),
  });
}

// ── Activity Feed ──────────────────────────────────────────────

export interface ActivityEvent {
  kind: 'state_change' | 'user_action' | 'command';
  at: string;
  component_id?: string;
  component_name?: string;
  // state_change fields
  from_state?: string;
  to_state?: string;
  trigger?: string;
  // user_action fields
  user?: string;
  action?: string;
  details?: Record<string, unknown>;
  // command fields
  request_id?: string;
  command_type?: string;
  exit_code?: number | null;
  duration_ms?: number | null;
  dispatched_at?: string;
  completed_at?: string | null;
}

export interface HealthSummary {
  total_components: number;
  state_breakdown: Array<{ state: string; count: number }>;
  error_components: Array<{
    component_id: string;
    name: string;
    state: string;
    since: string;
  }>;
  agents: Array<{
    agent_id: string;
    hostname: string;
    active: boolean;
    last_heartbeat: string | null;
    stale: boolean;
  }>;
  recent_incidents: Array<{
    component_id: string;
    component_name: string;
    from_state: string;
    at: string;
  }>;
}

export function useActivityFeed(appId: string, limit = 50) {
  return useQuery({
    queryKey: ['apps', appId, 'activity'],
    queryFn: async () => {
      const { data } = await client.get<{ events: ActivityEvent[] }>(
        `/apps/${appId}/activity?limit=${limit}`,
      );
      return data.events;
    },
    enabled: !!appId,
    refetchInterval: 15_000,
  });
}

export function useHealthSummary(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'health-summary'],
    queryFn: async () => {
      const { data } = await client.get<HealthSummary>(
        `/apps/${appId}/health-summary`,
      );
      return data;
    },
    enabled: !!appId,
    refetchInterval: 10_000,
  });
}

// ── Import ─────────────────────────────────────────────────────

export interface ImportResult {
  application_id: string;
  application_name: string;
  components_created: number;
  groups_created: number;
  variables_created: number;
  commands_created: number;
  dependencies_created: number;
  links_created: number;
  warnings: string[];
}

export function useImportYaml() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { yaml: string; site_id: string }) => {
      const { data } = await client.post<ImportResult>('/import/yaml', payload);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['apps'] }),
  });
}

export function useImportJson() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { json: string; site_id: string }) => {
      const { data } = await client.post<ImportResult>('/import/json', payload);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['apps'] }),
  });
}

// ── Export ─────────────────────────────────────────────────────

export interface ExportedApplication {
  format_version: string;
  exported_at: string;
  application: {
    name: string;
    description: string | null;
    tags: string[];
    variables: Array<{
      name: string;
      value: string;
      description: string | null;
      is_secret: boolean;
    }>;
    groups: Array<{
      name: string;
      description: string | null;
      color: string | null;
      display_order: number;
    }>;
    components: Array<{
      name: string;
      display_name: string | null;
      description: string | null;
      type: string;
      icon: string | null;
      group: string | null;
      host: string | null;
      commands: Record<string, { cmd: string; timeout_seconds?: number } | undefined>;
      custom_commands: Array<{
        name: string;
        command: string;
        description: string | null;
        requires_confirmation: boolean;
        parameters: Array<{
          name: string;
          description: string | null;
          default_value: string | null;
          validation_regex: string | null;
          required: boolean;
          param_type: string;
          enum_values: string[] | null;
        }>;
      }>;
      links: Array<{
        label: string;
        url: string;
        link_type: string;
      }>;
      position_x: number | null;
      position_y: number | null;
      check_interval_seconds: number;
      start_timeout_seconds: number;
      stop_timeout_seconds: number;
      is_optional: boolean;
    }>;
    dependencies: Array<{
      from: string;
      to: string;
      dep_type?: string;
    }>;
  };
}

export function useExportApp(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'export'],
    queryFn: async () => {
      const { data } = await client.get<ExportedApplication>(`/apps/${appId}/export`);
      return data;
    },
    enabled: false, // Manual fetch only
  });
}

export function useExportAppMutation() {
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.get<ExportedApplication>(`/apps/${appId}/export`);
      return data;
    },
  });
}

// ── Component Position Updates (for Map Designer) ──────────────

export function useUpdateComponentPosition() {
  return useMutation({
    mutationFn: async (payload: { componentId: string; x: number; y: number }) => {
      await client.patch(`/components/${payload.componentId}/position`, {
        x: payload.x,
        y: payload.y,
      });
    },
  });
}

export function useUpdateComponentPositions() {
  return useMutation({
    mutationFn: async (positions: Array<{ id: string; x: number; y: number }>) => {
      await client.patch('/components/batch-positions', { positions });
    },
  });
}

// ── Delete Dependency ──────────────────────────────────────────

export function useDeleteDependency() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; dependency_id: string }) => {
      await client.delete(`/dependencies/${payload.dependency_id}`);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id] }),
  });
}

// ── Update Component ───────────────────────────────────────────

export function useUpdateComponent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      id: string;
      app_id: string;
      name?: string;
      display_name?: string;
      description?: string;
      component_type?: string;
      icon?: string;
      host?: string;
      group_id?: string | null;
      check_cmd?: string;
      start_cmd?: string;
      stop_cmd?: string;
      cluster_size?: number | null;
      cluster_nodes?: string[] | null;
    }) => {
      const { data } = await client.put(`/components/${payload.id}`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id] }),
  });
}

// ── Delete Component ───────────────────────────────────────────

export function useDeleteComponent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { id: string; app_id: string }) => {
      await client.delete(`/components/${payload.id}`);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id] }),
  });
}

// ── Component Metrics ──────────────────────────────────────────

export interface ComponentMetrics {
  component_id: string;
  metrics: Record<string, unknown> | null;
  exit_code: number;
  at: string;
}

export function useComponentMetrics(componentId: string | null) {
  return useQuery({
    queryKey: ['components', componentId, 'metrics'],
    queryFn: async () => {
      const { data } = await client.get<ComponentMetrics>(`/components/${componentId}/metrics`);
      return data;
    },
    enabled: !!componentId,
    refetchInterval: 30000, // Refetch every 30 seconds
  });
}

export function useComponentMetricsHistory(componentId: string | null) {
  return useQuery({
    queryKey: ['components', componentId, 'metrics', 'history'],
    queryFn: async () => {
      const { data } = await client.get<{ history: ComponentMetrics[] }>(`/components/${componentId}/metrics/history`);
      return data.history;
    },
    enabled: !!componentId,
  });
}
