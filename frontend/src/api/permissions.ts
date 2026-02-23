import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface AppPermission {
  id: string;
  app_id?: string;
  user_id?: string;
  team_id?: string;
  level: string;
  user_email?: string;
  team_name?: string;
  type: 'user' | 'team';
  expires_at?: string | null;
}

export interface ShareLink {
  id: string;
  app_id?: string;
  token: string;
  permission_level: string;
  expires_at: string | null;
  max_uses: number | null;
  current_uses: number;
  created_by?: string;
}

export interface ShareLinkInfo {
  app_id: string;
  app_name: string;
  permission_level: string;
  expired: boolean;
  exhausted: boolean;
  valid: boolean;
}

export function useAppPermissions(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'permissions'],
    queryFn: async () => {
      const { data } = await client.get<{ permissions: AppPermission[] }>(`/apps/${appId}/permissions`);
      return data.permissions;
    },
    enabled: !!appId,
  });
}

export function useEffectivePermission(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'permissions', 'effective'],
    queryFn: async () => {
      const { data } = await client.get<{ permission_level: string }>(`/apps/${appId}/permissions/effective`);
      return data.permission_level;
    },
    enabled: !!appId,
  });
}

export function useSetPermission() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; user_id?: string; team_id?: string; level: string }) => {
      if (payload.team_id) {
        const { data } = await client.post(`/apps/${payload.app_id}/permissions/teams`, {
          team_id: payload.team_id,
          permission_level: payload.level,
        });
        return data;
      }
      const { data } = await client.post(`/apps/${payload.app_id}/permissions/users`, {
        user_id: payload.user_id,
        permission_level: payload.level,
      });
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
      const { data } = await client.get<{ share_links: ShareLink[] }>(`/apps/${appId}/permissions/share-links`);
      return data.share_links;
    },
    enabled: !!appId,
  });
}

export function useCreateShareLink() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; permission_level: string; expires_at?: string; max_uses?: number }) => {
      const { data } = await client.post<ShareLink>(`/apps/${payload.app_id}/permissions/share-links`, payload);
      return data;
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'share-links'] }),
  });
}

export function useRevokeShareLink() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { app_id: string; link_id: string }) => {
      await client.delete(`/apps/${payload.app_id}/permissions/share-links/${payload.link_id}`);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['apps', vars.app_id, 'share-links'] }),
  });
}

export function useConsumeShareLink() {
  return useMutation({
    mutationFn: async (token: string) => {
      const { data } = await client.post<{ app_id: string; permission_level: string; status: string }>('/share-links/consume', { token });
      return data;
    },
  });
}

export function useShareLinkInfo(token: string) {
  return useQuery({
    queryKey: ['share-link-info', token],
    queryFn: async () => {
      // This endpoint is outside /api/v1 auth, so call directly
      const { data } = await client.get<ShareLinkInfo>(`../share/${token}`);
      return data;
    },
    enabled: !!token,
    retry: false,
  });
}
