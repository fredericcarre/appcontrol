import { useQuery } from '@tanstack/react-query';
import client from './client';

export interface SiteOverride {
  id: string;
  component_id: string;
  site_id: string;
  site_name: string;
  site_code: string;
  site_type: 'primary' | 'dr' | 'staging' | 'development';
  site_is_active: boolean;
  agent_id_override: string | null;
  override_agent_hostname: string | null;
  check_cmd_override: string | null;
  start_cmd_override: string | null;
  stop_cmd_override: string | null;
  rebuild_cmd_override: string | null;
  env_vars_override: Record<string, string> | null;
}

export interface SiteInfo {
  id: string;
  name: string;
  code: string;
  site_type: string;
}

export interface SiteOverridesResponse {
  overrides: SiteOverride[];
  primary_site: SiteInfo | null;
}

/**
 * Fetches all site overrides for components in an application.
 * Used by multi-site visualization to show per-site panels on component nodes.
 */
export function useSiteOverrides(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'site-overrides'],
    queryFn: async () => {
      const { data } = await client.get<SiteOverridesResponse>(
        `/apps/${appId}/site-overrides`,
      );
      return data;
    },
    enabled: !!appId,
    staleTime: 30_000,
  });
}

/**
 * Helper: Group site overrides by component_id for easy lookup.
 */
export function groupOverridesByComponent(
  overrides: SiteOverride[],
): Map<string, SiteOverride[]> {
  const map = new Map<string, SiteOverride[]>();
  for (const o of overrides) {
    const existing = map.get(o.component_id);
    if (existing) {
      existing.push(o);
    } else {
      map.set(o.component_id, [o]);
    }
  }
  return map;
}
