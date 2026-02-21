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
