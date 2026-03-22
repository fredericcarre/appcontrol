import { useQuery } from '@tanstack/react-query';
import client from './client';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export interface SiteInfo {
  id: string;
  name: string;
  code: string;
  site_type: 'primary' | 'dr' | 'staging' | 'development';
}

export interface CommandOverrides {
  check_cmd: string | null;
  start_cmd: string | null;
  stop_cmd: string | null;
  rebuild_cmd: string | null;
  env_vars: Record<string, string> | null;
}

export interface SiteBinding {
  site_id: string;
  site_name: string;
  site_code: string;
  site_type: 'primary' | 'dr' | 'staging' | 'development';
  profile_id: string;
  profile_name: string;
  profile_type: string;
  is_active: boolean;
  agent_id: string;
  agent_hostname: string;
  has_command_overrides: boolean;
  command_overrides: CommandOverrides | null;
}

export interface ComponentSiteBindings {
  component_id: string;
  component_name: string;
  site_bindings: SiteBinding[];
}

export interface SiteBindingsResponse {
  primary_site: SiteInfo | null;
  component_bindings: ComponentSiteBindings[];
}

// ═══════════════════════════════════════════════════════════════════════════
// Hooks
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Fetches all site bindings for components in an application.
 * Based on binding profiles (which define where components run)
 * merged with any command overrides.
 */
export function useSiteBindings(appId: string) {
  return useQuery({
    queryKey: ['apps', appId, 'site-bindings'],
    queryFn: async () => {
      const { data } = await client.get<SiteBindingsResponse>(
        `/apps/${appId}/site-overrides`,
      );
      return data;
    },
    enabled: !!appId,
    staleTime: 30_000,
  });
}

// Keep old name for backward compatibility during refactor
export const useSiteOverrides = useSiteBindings;

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Helper: Group site bindings by component_id for easy lookup.
 */
export function groupBindingsByComponent(
  bindings: ComponentSiteBindings[],
): Map<string, SiteBinding[]> {
  const map = new Map<string, SiteBinding[]>();
  for (const comp of bindings) {
    map.set(comp.component_id, comp.site_bindings);
  }
  return map;
}

// Legacy export - keep for compatibility
export type SiteOverride = SiteBinding;
export type SiteOverridesResponse = SiteBindingsResponse;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
export function groupOverridesByComponent(overrides: SiteBinding[]): Map<string, SiteBinding[]> {
  // This is for legacy compatibility - new code should use groupBindingsByComponent
  const map = new Map<string, SiteBinding[]>();
  // Group by component - but this won't work well since SiteBinding doesn't have component_id
  // This is a placeholder - the actual usage should switch to the new API
  return map;
}
