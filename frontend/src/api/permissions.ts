import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface AppPermission {
  id: string;
  app_id: string;
  user_id?: string;
  team_id?: string;
  level: string;
  user_email?: string;
  team_name?: string;
}

export interface ShareLink {
  id: string;
  app_id: string;
  token: string;
  permission_level: string;
  expires_at: string | null;
  max_uses: number | null;
  current_uses: number;
  created_by: string;
}

export function useAppPermissions(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'permissions'],
    queryFn: async () => {
      const { data } = await client.get<AppPermission[]>(`/apps/${appId}/permissions`);
      return data;
    },
    enabled: !!appId,
  });
}

export function useEffectivePermission(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'permissions', 'effective'],
    queryFn: async () => {
      const { data } = await client.get<{ level: string }>(`/apps/${appId}/permissions/effective`);
      return data.level;
    },
    enabled: !!appId,
  });
}

export function useSetPermission() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; user_id?: string; team_id?: string; level: string }) => {
      const { data } = await client.post(`/apps/${payload.app_id}/permissions`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'permissions'] }),
  });
}

export function useRemovePermission() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; permission_id: string }) => {
      await client.delete(`/apps/${payload.app_id}/permissions/${payload.permission_id}`);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'permissions'] }),
  });
}

export function useShareLinks(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'share-links'],
    queryFn: async () => {
      const { data } = await client.get<ShareLink[]>(`/apps/${appId}/share-links`);
      return data;
    },
    enabled: !!appId,
  });
}

export function useCreateShareLink() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; permission_level: string; expires_at?: string; max_uses?: number }) => {
      const { data } = await client.post<ShareLink>(`/apps/${payload.app_id}/share-links`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'share-links'] }),
  });
}
