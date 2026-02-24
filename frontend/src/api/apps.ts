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
  component_count: number;
  created_at: string;
  updated_at: string;
}

export interface ApplicationDetail extends Application {
  components: Component[];
  dependencies: Dependency[];
}

export interface Component {
  id: string;
  app_id: string;
  name: string;
  display_name: string | null;
  description: string | null;
  icon: string | null;
  group_id: string | null;
  host: string;
  component_type: string;
  state: string;
  check_cmd: string | null;
  start_cmd: string | null;
  stop_cmd: string | null;
  restart_cmd: string | null;
  check_interval_secs: number;
  agent_id: string | null;
  group_name: string | null;
  display_order: number;
  position_x: number | null;
  position_y: number | null;
  is_protected: boolean;
  created_at: string;
  updated_at: string;
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
      const { data } = await client.get<Application[]>('/apps');
      return data;
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

export function useStartApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post(`/apps/${appId}/start`);
      return data;
    },
    onSuccess: (_, appId) => qc.invalidateQueries({ queryKey: ['apps', appId] }),
  });
}

export function useStopApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (appId: string) => {
      const { data } = await client.post(`/apps/${appId}/stop`);
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
    mutationFn: async (payload: Partial<Component> & { app_id: string; name: string; host: string }) => {
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

// ── YAML Import ────────────────────────────────────────────────

export function useImportYaml() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { yaml: string; site_id: string }) => {
      const { data } = await client.post('/import/yaml', payload);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['apps'] }),
  });
}
