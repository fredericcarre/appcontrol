import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Site {
  id: string;
  organization_id: string;
  name: string;
  code: string;
  site_type: 'primary' | 'dr' | 'staging' | 'development';
  location: string | null;
  is_active: boolean;
  created_at: string;
}

export function useSites() {
  return useQuery({
    queryKey: ['sites'],
    queryFn: async () => {
      const { data } = await client.get<{ sites: Site[] }>('/sites');
      return data.sites;
    },
  });
}

export function useSite(siteId: string) {
  return useQuery({
    queryKey: ['sites', siteId],
    queryFn: async () => {
      const { data } = await client.get<Site>(`/sites/${siteId}`);
      return data;
    },
    enabled: !!siteId,
  });
}

export function useCreateSite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { name: string; code: string; site_type: string; location?: string }) => {
      const { data } = await client.post<Site>('/sites', payload);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sites'] });
    },
  });
}

export function useUpdateSite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, ...payload }: { id: string; name?: string; code?: string; site_type?: string; location?: string; is_active?: boolean }) => {
      const { data } = await client.put<Site>(`/sites/${id}`, payload);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sites'] });
    },
  });
}

export function useDeleteSite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      await client.delete(`/sites/${id}`);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sites'] });
    },
  });
}
