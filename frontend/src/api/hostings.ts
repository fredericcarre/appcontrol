import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Hosting {
  id: string;
  organization_id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
}

export interface HostingSite {
  id: string;
  name: string;
  code: string;
  site_type: 'primary' | 'dr' | 'staging' | 'development';
  location: string | null;
  is_active: boolean;
}

export function useHostings() {
  return useQuery({
    queryKey: ['hostings'],
    queryFn: async () => {
      const { data } = await client.get<{ hostings: Hosting[] }>('/hostings');
      return data.hostings;
    },
  });
}

export function useHosting(hostingId: string) {
  return useQuery({
    queryKey: ['hostings', hostingId],
    queryFn: async () => {
      const { data } = await client.get<Hosting>(`/hostings/${hostingId}`);
      return data;
    },
    enabled: !!hostingId,
  });
}

export function useHostingSites(hostingId: string) {
  return useQuery({
    queryKey: ['hostings', hostingId, 'sites'],
    queryFn: async () => {
      const { data } = await client.get<{ sites: HostingSite[] }>(
        `/hostings/${hostingId}/sites`
      );
      return data.sites;
    },
    enabled: !!hostingId,
  });
}

export function useCreateHosting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { name: string; description?: string }) => {
      const { data } = await client.post<Hosting>('/hostings', payload);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['hostings'] });
    },
  });
}

export function useUpdateHosting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({
      id,
      ...payload
    }: {
      id: string;
      name?: string;
      description?: string;
    }) => {
      const { data } = await client.put<Hosting>(`/hostings/${id}`, payload);
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['hostings'] });
    },
  });
}

export function useDeleteHosting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      await client.delete(`/hostings/${id}`);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['hostings'] });
    },
  });
}
