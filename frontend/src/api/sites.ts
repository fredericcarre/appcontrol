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

// ============================================================================
// Site Overrides - Per-component failover configuration
// ============================================================================

export interface SiteOverride {
  id: string;
  component_id: string;
  site_id: string;
  site_name?: string;
  site_code?: string;
  agent_id_override: string | null;
  agent_hostname?: string | null;
  check_cmd_override: string | null;
  start_cmd_override: string | null;
  stop_cmd_override: string | null;
  rebuild_cmd_override: string | null;
  env_vars_override: Record<string, string> | null;
  created_at: string;
}

export interface SiteOverrideInput {
  site_id: string;
  agent_id_override?: string | null;
  check_cmd_override?: string | null;
  start_cmd_override?: string | null;
  stop_cmd_override?: string | null;
  rebuild_cmd_override?: string | null;
  env_vars_override?: Record<string, string> | null;
}

/**
 * Get all site overrides for a component
 */
export function useComponentSiteOverrides(componentId: string) {
  return useQuery({
    queryKey: ['components', componentId, 'site-overrides'],
    queryFn: async () => {
      const { data } = await client.get<{ overrides: SiteOverride[] }>(
        `/components/${componentId}/site-overrides`
      );
      return data.overrides;
    },
    enabled: !!componentId,
  });
}

/**
 * Create or update a site override for a component
 */
export function useUpsertSiteOverride(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: SiteOverrideInput) => {
      const { data } = await client.put<SiteOverride>(
        `/components/${componentId}/site-overrides/${payload.site_id}`,
        payload
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['components', componentId, 'site-overrides'] });
    },
  });
}

/**
 * Delete a site override for a component
 */
export function useDeleteSiteOverride(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (siteId: string) => {
      await client.delete(`/components/${componentId}/site-overrides/${siteId}`);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['components', componentId, 'site-overrides'] });
    },
  });
}
