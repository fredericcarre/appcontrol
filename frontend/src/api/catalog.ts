import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface CatalogEntry {
  id: string;
  org_id: string;
  type_key: string;
  label: string;
  description: string | null;
  icon: string;
  color: string;
  category: string | null;
  default_check_cmd: string | null;
  default_start_cmd: string | null;
  default_stop_cmd: string | null;
  default_env_vars: Record<string, string> | null;
  display_order: number;
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
}

export function useCatalog() {
  return useQuery({
    queryKey: ['catalog', 'component-types'],
    queryFn: async () => {
      const { data } = await client.get<{ entries: CatalogEntry[] }>(
        '/catalog/component-types',
      );
      return data.entries;
    },
    staleTime: 5 * 60 * 1000, // cache for 5 min — catalog changes rarely
  });
}

export function useCreateCatalogEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      type_key: string;
      label: string;
      description?: string;
      icon?: string;
      color?: string;
      category?: string;
      default_check_cmd?: string;
      default_start_cmd?: string;
      default_stop_cmd?: string;
      default_env_vars?: Record<string, string>;
      display_order?: number;
    }) => {
      const { data } = await client.post<CatalogEntry>(
        '/catalog/component-types',
        payload,
      );
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['catalog'] }),
  });
}

export function useUpdateCatalogEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      id: string;
      label?: string;
      description?: string;
      icon?: string;
      color?: string;
      category?: string;
      default_check_cmd?: string;
      default_start_cmd?: string;
      default_stop_cmd?: string;
      default_env_vars?: Record<string, string>;
      display_order?: number;
    }) => {
      const { id, ...body } = payload;
      const { data } = await client.put<CatalogEntry>(
        `/catalog/component-types/${id}`,
        body,
      );
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['catalog'] }),
  });
}

export function useDeleteCatalogEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      await client.delete(`/catalog/component-types/${id}`);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['catalog'] }),
  });
}

export function useImportCatalog() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (entries: Array<{
      type_key: string;
      label: string;
      description?: string;
      icon?: string;
      color?: string;
      category?: string;
      default_check_cmd?: string;
      default_start_cmd?: string;
      default_stop_cmd?: string;
      default_env_vars?: Record<string, string>;
      display_order?: number;
    }>) => {
      const { data } = await client.post<{ created: number; skipped: number; total: number }>(
        '/catalog/component-types/import',
        { entries },
      );
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['catalog'] }),
  });
}

export function useSeedCatalog() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { data } = await client.post<{ seeded: number }>(
        '/catalog/component-types/seed',
      );
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['catalog'] }),
  });
}
